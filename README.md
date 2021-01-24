# Compiler-Autograder

## Requirements?

- Assignment accompanied with a CMakeLists.txt
- Source code compressed in `gzip tar` format
- The server need to prepare a `gpg` environment
- The server need to have docker `csc3050` installed and accessible by the running user

## Where to find the docker used by this repo?

DockerHub: Shrodingerzhu/csc3050

## How to run server?

```
cargo run -- -t test.json
```

## How to run client?

```
tar cvzf src.tgz YOUR_SRC_PATH/* # yes, CMakeLists.txt directly in the archive, rather than in any subdirectory
python client.py src.tgz
```
