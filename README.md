# Compiler-Autograder

## Requirements?

- Assignment accompanied with a CMakeLists.txt
- Source code compressed in `gzip tar` format
- The server need to prepare a `gpg` environment
- The server need to have docker `csc3050` installed and accessible by the running user

## Where to find the docker used by this repo?

DockerHub: shrodingerzhu/csc3050

## How to run server?

```
RUST_LOG=info cargo run -- -t test.json -i csc3050 -l 0.0.0.0 -p8080
```

## How to run client?

```
# pip3 install websocket_client
tar cvzf src.tgz YOUR_SRC_PATH/* # yes, CMakeLists.txt directly in the archive, rather than in any subdirectory
python client.py src.tgz ws://ip:port
```
