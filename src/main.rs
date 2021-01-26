use actix::{Actor, StreamHandler};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use actix_web::web::Bytes;
use std::fs::File;
use log::*;
use std::io::Write;
use actix_web_actors::ws::WebsocketContext;
use sha2::Digest;
use lazy_static::*;
use structopt::StructOpt;
use anyhow::*;
use std::process::Stdio;

lazy_static! {
    static ref CONF : Config = Config::from_args();
    static ref TESTS : Arc<TestCases> = Arc::new({
        simd_json::from_str(&mut std::fs::read_to_string(&CONF.tests).unwrap()).unwrap()
    });
}

#[derive(Copy, Clone)]
enum Status {
    Start,
    Docker,
    Compile,
    Run(usize),
    GPG,
    Finished,
}

/// Define HTTP actor
struct Runner {
    image: String,
    test_cases: Arc<Vec<TestCase>>,
    file_path: Option<TempDir>,
    docker: Option<String>,
    checksum: Option<String>,
    report: String,
    score: usize,
    status: Status,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TestCase {
    name: String,
    source: PathBuf,
    input_file: PathBuf,
    output_file: PathBuf,
    score: usize,
}

impl<'a> Actor for Runner {
    type Context = ws::WebsocketContext<Self>;
}


impl Runner {
    fn recv_file(&mut self, bin: Bytes, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let mut path = dir.path().to_path_buf();
        debug!("created temp dir: {}", path.display());
        path.push("src.tgz");
        let mut file = File::create(path)?;
        let data = bin.to_vec();
        let mut checksum = sha2::Sha512::default();
        checksum.update(&data);
        self.checksum.replace(hex::encode(checksum.finalize()));
        file.write(&data)?;
        file.flush()?;
        drop(file);
        self.file_path.replace(dir);
        ctx.text("[file received]\n");
        self.status = Status::Docker;
        Ok(())
    }

    fn start_docker(&mut self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        let output = std::process::Command::new("docker")
            .arg("create")
            .arg(&self.image)
            .output()?;
        self.docker.replace(String::from_utf8(output.stdout)?.trim().to_string());
        ctx.text(format!("created docker: {}\n", self.docker.as_ref().ok_or(anyhow!("illegal operation"))?));
        if std::process::Command::new("docker")
            .arg("start")
            .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
            .stdout(Stdio::null())
            .status()?
            .success() {
            ctx.text("docker started\n");
        } else {
            std::process::Command::new("docker")
                .arg("rm")
                .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
                .status()?;
        }
        self.status = Status::Compile;
        Ok(())
    }
    fn compile(&mut self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        ctx.text("[copy files]");
        std::process::Command::new("docker")
            .current_dir(self.file_path.as_ref().ok_or(anyhow!("illegal operation"))?.path())
            .arg("cp")
            .arg("src.tgz")
            .arg(format!("{}:/tmp", self.docker.as_ref().ok_or(anyhow!("illegal operation"))?))
            .status()?;
        ctx.text("[untar files]");
        let untar = std::process::Command::new("docker")
            .arg("exec")
            .arg("-w")
            .arg("/tmp")
            .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
            .arg("tar")
            .arg("xzf")
            .arg("src.tgz")
            .output()?;
        ctx.text(format!("finished with {}", untar.status));
        ctx.text("[run cmake]");
        let cmake = std::process::Command::new("docker")
            .arg("exec")
            .arg("-w")
            .arg("/tmp")
            .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
            .arg("cmake")
            .arg(".")
            .arg("-G")
            .arg("Ninja")
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .output()?;
        ctx.text(String::from_utf8(cmake.stdout)?);
        ctx.text("[cmake stderr]");
        ctx.text(String::from_utf8(cmake.stderr)?);
        ctx.text("[build]");
        let build = std::process::Command::new("docker")
            .arg("exec")
            .arg("-w")
            .arg("/tmp")
            .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
            .arg("cmake")
            .arg("--build")
            .arg(".")
            .arg("--parallel")
            .arg("4")
            .output()?;
        ctx.text(String::from_utf8(build.stdout)?);
        ctx.text("[build stderr]");
        ctx.text(String::from_utf8(build.stderr)?);
        self.status = Status::Run(0);
        if !untar.status.success() || !cmake.status.success() || !build.status.success() {
            Err(anyhow!("compile"))
        } else {
            Ok(())
        }
    }

    fn run_cases(&mut self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        let i = match self.status {
            Status::Run(k) => Ok(&self.test_cases[k]),
            _ => Err(anyhow!("invalid status"))
        }?;
        ctx.text(format!("[compile {}]", i.name));
        let a = std::fs::read_to_string(&i.source)?;
        let b = std::fs::read_to_string(&i.input_file)?;
        let c = std::fs::read_to_string(&i.output_file)?;
        let mut run = std::process::Command::new("docker")
            .arg("exec")
            .arg("-w")
            .arg("/tmp")
            .arg("-i")
            .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
            .arg("/tmp/main")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        run.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.write(a.as_bytes())?;
        run.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.flush()?;
        run.stdin.take();
        let output = run.wait_with_output()?;
        ctx.text(format!("finished with {}", output.status));
        let asm = String::from_utf8(output.stdout)?;
        ctx.text(format!("[gcc {}]\n", i.name));
        let mut gcc = std::process::Command::new("docker")
            .arg("exec")
            .arg("-w")
            .arg("/tmp")
            .arg("-i")
            .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
            .arg("mcc")
            .arg("-static")
            .arg("-x")
            .arg("assembler")
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        gcc.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.write(asm.trim().as_bytes())?;
        gcc.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.flush()?;
        gcc.stdin.take();
        let output = gcc.wait_with_output()?;
        ctx.text(format!("finished with {}", output.status));
        ctx.text(format!("[gcc {} stderr]\n", i.name));
        ctx.text(String::from_utf8(output.stderr)?);
        ctx.text(format!("[qemu {}]\n", i.name));
        let mut execution = std::process::Command::new("docker")
            .arg("exec")
            .arg("-w")
            .arg("/tmp")
            .arg("-i")
            .arg(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?)
            .arg("qemu-mipsel")
            .arg("./a.out")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        execution.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.write(b.trim().as_bytes())?;
        execution.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.flush()?;
        execution.stdin.take();
        let output = execution.wait_with_output()?;
        let result = String::from_utf8(output.stdout)?;
        ctx.text(format!("[qemu {} return code]", i.name));
        ctx.text(output.status.to_string());
        ctx.text(format!("[qemu {} stderr]", i.name));
        ctx.text(String::from_utf8(output.stderr)?);
        let flag = result.trim() == c.trim();
        if flag {
            self.score = self.score + i.score;
        } else {
            ctx.text(format!("[{} output differences]", i.name));
            for diff in diff::lines(c.trim(), result.trim()) {
                match diff {
                    diff::Result::Left(l) => ctx.text(format!("- {}", l)),
                    diff::Result::Both(l, _) => ctx.text(format!("  {}", l)),
                    diff::Result::Right(r) => ctx.text(format!("+ {}", r))
                }
            }
        }
        let report = &mut self.report;
        report.push_str(format!("test {}, score: {}, success: {}\n", i.name, i.score, flag).as_str());
        if let Status::Run(cnt) = &mut self.status {
            *cnt += 1;
        }
        if let Status::Run(cnt) = self.status {
            if self.test_cases.len() <= cnt {
                self.status = Status::GPG;
            }
        }
        Ok(())
    }

    fn gpg(&mut self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        ctx.text("[gpg report]");
        self.report.push_str("datetime: ");
        self.report.push_str(chrono::Local::now().to_string().as_str());
        self.report.push('\n');
        self.report.push_str(format!("total score: {}\n", self.score).as_str());
        self.report.push_str("file checksum: ");
        self.report.push_str(self.checksum.as_ref().ok_or(anyhow!("illegal operation"))?);
        self.report.push('\n');
        self.report.push_str("docker uuid: ");
        self.report.push_str(self.docker.as_ref().ok_or(anyhow!("illegal operation"))?);
        self.report.push_str("\n\n");
        let mut gpg = std::process::Command::new("gpg")
            .arg("--clear-sign")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        gpg.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.write(self.report.as_bytes())?;
        gpg.stdin.as_ref().ok_or(anyhow!("illegal operation"))?.flush()?;
        gpg.stdin.take();
        let output = gpg.wait_with_output()?;
        ctx.text(String::from_utf8(output.stdout)?);
        self.status = Status::Finished;
        Ok(())
    }

    fn clean_up(&mut self) {
        if let Some(p) = self.file_path.take() {
            match std::fs::remove_dir_all(p.path()) {
                _ => ()
            }
        }

        if let Some(x) = self.docker.take() {
            match std::process::Command::new("docker")
                .arg("kill")
                .arg(&x)
                .stdout(Stdio::null())
                .status() {
                _ => ()
            }
            match std::process::Command::new("docker")
                .arg("rm")
                .arg(&x)
                .stdout(Stdio::null())
                .status() {
                _ => ()
            }
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.clean_up();
    }
}


/// Handler for ws::Message message
impl<'a> StreamHandler<Result<ws::Message, ws::ProtocolError>> for Runner {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        match msg {
            Ok(ws::Message::Binary(bin)) => match self.recv_file(bin, ctx) {
                Ok(()) => ctx.ping("next".as_bytes()),
                Err(e) => {
                    ctx.text(e.to_string());
                    ctx.close(None);
                    self.clean_up();
                }
            }
            Ok(ws::Message::Pong(_)) => {
                let result = match self.status {
                    Status::Docker => self.start_docker(ctx),
                    Status::Compile => self.compile(ctx),
                    Status::Run(_) => self.run_cases(ctx),
                    Status::GPG => self.gpg(ctx),
                    Status::Finished => {
                        ctx.text("Finished");
                        ctx.close(None);
                        self.clean_up();
                        Ok(())
                    }
                    _ => Err(anyhow!("[error: no such command]"))
                };
                match result {
                    Ok(()) => ctx.ping("next".as_bytes()),
                    Err(e) => {
                        ctx.text(e.to_string());
                        ctx.close(None);
                    }
                }
            }
            Err(e) => {
                ctx.text(e.to_string());
                ctx.close(None);
            }
            _ => {
                ctx.close(None);
            }
        }
    }
}

async fn index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let actor = Runner {
        image: CONF.image.clone(),
        test_cases: TESTS.clone(),
        file_path: None,
        docker: None,
        checksum: None,
        report: String::new(),
        score: 0,
        status: Status::Start,
    };
    let mut res = ws::handshake(&req)?;
    let resp = res.streaming(WebsocketContext::with_codec(actor, stream, actix_http::ws::Codec::new().max_size(64 * 1024 * 1024)));
    println!("{:?}", resp);
    Ok(resp)
}

type TestCases = Vec<TestCase>;

#[derive(structopt::StructOpt)]
struct Config {
    #[structopt(short, long, help = "Path to the json file containing test cases")]
    tests: PathBuf,
    #[structopt(short, long, help = "Docker image name")]
    image: String,
    #[structopt(short = "l", long, help = "Listen IP address")]
    ip: std::net::IpAddr,
    #[structopt(short, long, help = "Listen port")]
    port: u16,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    pretty_env_logger::init_timed();
    info!("tests from: {}, size: {}", CONF.tests.display(), TESTS.len());
    HttpServer::new(|| App::new().route("/", web::get().to(index)))
        .bind(format!("{}:{}", CONF.ip, CONF.port))?
        .run()
        .await
}

