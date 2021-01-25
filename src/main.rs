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

lazy_static! {
    static ref CONF : Config = Config::from_args();
    static ref TESTS : Arc<TestCases> = Arc::new({
        simd_json::from_str(&mut std::fs::read_to_string(&CONF.tests).unwrap()).unwrap()
    });
}
/// Define HTTP actor
struct Runner {
    image: String,
    test_cases: Arc<Vec<TestCase>>,
    file_path: Option<TempDir>,
    docker: Option<String>,
    checksum: Option<String>
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TestCase {
    name: String,
    source: PathBuf,
    input_file: PathBuf,
    output_file: PathBuf,
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
        let mut       checksum = sha2::Sha512::default();
        checksum.update(&data);
        self.checksum.replace(hex::encode(checksum.finalize()));
        file.write(&data)?;
        file.flush()?;
        drop(file);
        self.file_path.replace(dir);
        ctx.text("[file received]\n");
        Ok(())
    }

    fn start_docker(&mut self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        let output = std::process::Command::new("docker")
            .arg("create")
            .arg(&self.image)
            .output()?;
        self.docker.replace(String::from_utf8(output.stdout)?.trim().to_string());
        ctx.text(format!("created docker: {}\n", self.docker.as_ref().unwrap()));
        if std::process::Command::new("docker")
            .arg("start")
            .arg(self.docker.as_ref().unwrap())
            .status()?
            .success() {
            ctx.text("docker started\n");
        } else {
            std::process::Command::new("docker")
                .arg("rm")
                .arg(self.docker.as_ref().unwrap())
                .status()?;
        }
        Ok(())
    }
    fn compile(&mut self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<()> {
        ctx.text("[copy files]");
        std::process::Command::new("docker")
            .current_dir( self.file_path.as_ref().unwrap().path())
            .arg("cp")
            .arg("src.tgz")
            .arg(format!("{}:/tmp", self.docker.as_ref().unwrap()))
            .status()?;
        ctx.text("[untar files]");
        let untar = std::process::Command::new("docker")
            .arg( "exec")
            .arg("-w")
            .arg("/tmp")
            .arg(self.docker.as_ref().unwrap())
            .arg("tar")
            .arg("xzf")
            .arg("src.tgz")
            .output()?;
        ctx.text(format!("finished with {}", untar.status));
        ctx.text("[run cmake]");
        let cmake = std::process::Command::new("docker")
            .arg( "exec")
            .arg("-w")
            .arg("/tmp")
            .arg(self.docker.as_ref().unwrap())
            .arg("cmake")
            .arg(".")
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .output()?;
        ctx.text(String::from_utf8(cmake.stdout)?);
        ctx.text("[cmake stderr]");
        ctx.text(String::from_utf8(cmake.stderr)?);
        ctx.text("[build]");
        let build = std::process::Command::new("docker")
            .arg( "exec")
            .arg("-w")
            .arg("/tmp")
            .arg(self.docker.as_ref().unwrap())
            .arg("cmake")
            .arg("--build")
            .arg(".")
            .arg("--parallel")
            .arg("4")
            .output()?;
        ctx.text(String::from_utf8(build.stdout)?);
        ctx.text("[build stderr]");
        ctx.text(String::from_utf8(build.stderr)?);
        Ok(())
    }

    fn run_cases(&mut self, ctx: &mut <Self as Actor>::Context) -> anyhow::Result<String> {
        let mut results = Vec::new();
        for i in self.test_cases.iter() {
            ctx.text(format!("[compile {}]", i.name));
            let a = std::fs::read_to_string(&i.source)?;
            let b = std::fs::read_to_string(&i.input_file)?;
            let c = std::fs::read_to_string(&i.output_file)?;
            let mut run = std::process::Command::new("docker")
                .arg( "exec")
                .arg("-w")
                .arg("/tmp")
                .arg("-i")
                .arg(self.docker.as_ref().unwrap())
                .arg("/tmp/main")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            run.stdin.as_ref().unwrap().write(a.as_bytes())?;
            run.stdin.as_ref().unwrap().flush()?;
            run.stdin.take();
            let output = run.wait_with_output()?;
            ctx.text(format!("finished with {}", output.status));
            let asm = String::from_utf8(output.stdout)?;
            ctx.text(format!("[gcc {}]\n", i.name));
            let mut gcc = std::process::Command::new("docker")
                .arg( "exec")
                .arg("-w")
                .arg("/tmp")
                .arg("-i")
                .arg(self.docker.as_ref().unwrap())
                .arg("mcc")
                .arg("-static")
                .arg("-x")
                .arg("assembler")
                .arg("-")
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()?;
            gcc.stdin.as_ref().unwrap().write(asm.trim().as_bytes())?;
            gcc.stdin.as_ref().unwrap().flush()?;
            gcc.stdin.take();
            let output = gcc.wait_with_output()?;
            ctx.text(format!("finished with: {}", output.status));
            ctx.text(format!("[gcc {} stderr]\n{}", i.name, String::from_utf8(output.stderr)?));
            ctx.text(format!("[qemu {}]\n", i.name));
            let mut execution = std::process::Command::new("docker")
                .arg( "exec")
                .arg("-w")
                .arg("/tmp")
                .arg("-i")
                .arg(self.docker.as_ref().unwrap())
                .arg("qemu-mipsel")
                .arg("./a.out")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            execution.stdin.as_ref().unwrap().write(b.trim().as_bytes())?;
            execution.stdin.as_ref().unwrap().flush()?;
            execution.stdin.take();
            let output = execution.wait_with_output()?;
            let result = String::from_utf8(output.stdout)?;
            ctx.text(format!("[qemu {} return code]\n{}", i.name, output.status));
            ctx.text(format!("[qemu {} stderr]\n{}", i.name, String::from_utf8(output.stderr)?));
            let flag = result.trim() == c.trim();
            results.push(flag);
        }
        let mut report = String::new();
        report.push_str("file checksum: ");
        report.push_str(self.checksum.as_ref().unwrap());
        report.push('\n');
        report.push_str("docker uuid: ");
        report.push_str(self.docker.as_ref().unwrap());
        report.push_str("\n\n");

        for i in 0..results.len() {
            report.push_str(format!("test #{}, name: {}, success: {}\n", i, self.test_cases[i].name, results[i]).as_str());
        }
        Ok(report)
    }

    fn gpg(&mut self, ctx: &mut <Self as Actor>::Context, report: &str) -> anyhow::Result<()> {
        ctx.text("[gpg report]");
        let mut gpg = std::process::Command::new("gpg")
            .arg( "--clear-sign")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        gpg.stdin.as_ref().unwrap().write(report.as_bytes())?;
        gpg.stdin.as_ref().unwrap().flush()?;
        gpg.stdin.take();
        let output = gpg.wait_with_output()?;
        ctx.text(String::from_utf8(output.stdout)?);
        Ok(())
    }

    fn clean_up(&mut self) {
        if let Some (p) = self.file_path.take() {
            match std::fs::remove_dir_all(p.path()) {
                _ => ()
            }
        }

        if let Some(x) = & self.docker {
            match std::process::Command::new("docker")
                .arg("kill")
                .arg(x)
            .status() {
                _ => ()
            }
            match std::process::Command::new("docker")
                .arg("rm")
                .arg(x)
                .status() {
                _ => ()
            }
        }
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
            Ok(ws::Message::Binary(bin)) => match self.recv_file(bin, ctx)
                .and_then(|_|self.start_docker(ctx))
                .and_then(|_|self.compile(ctx))
                .and_then(|_|self.run_cases(ctx))
                .and_then(|r|self.gpg(ctx, &r)){
                Ok(_) => {
                    ctx.text("Finished");
                }
                Err(e) => ctx.text(e.to_string()),
            },
            Err(e) => ctx.text(e.to_string()),
            _ => error!("illegal formal {:#?}", msg),
        }
        self.clean_up();
    }
}

async fn index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let actor = Runner { image: CONF.image.clone(), test_cases: TESTS.clone(), file_path: None, docker: None, checksum: None };
    let mut res = ws::handshake(&req)?;
    let resp = res.streaming(WebsocketContext::with_codec(actor, stream, actix_http::ws::Codec::new().max_size(512 * 1024 * 1024)));
    println!("{:?}", resp);
    Ok(resp)
}

type TestCases = Vec<TestCase>;

#[derive(structopt::StructOpt)]
struct Config {
    #[structopt(short, long, about="path to the json file containing test cases")]
    tests: PathBuf,
    #[structopt(short, long, about="docker image name")]
    image: String,
    #[structopt(short="l", long, about="listen ip")]
    ip: std::net::IpAddr,
    #[structopt(short, long, about="listen port")]
    port: u8,
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

