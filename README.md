# Comipiler-Autograder

## Requirements?

- Assignment accompanied with a CMakeLists.txt
- Source code compressed in `gzip tar` format

## Where to find the docker used by this repo?

DockerHub: Shrodingerzhu/csc3050

## How to run server?

The server need to prepare a `gpg` environment.
Then,
```
cargo run -- -t test.json
```

## How to run client?

```
tar cvzf src.tgz YOUR_SRC_PATH/* # yes, CMakeLists.txt directly in the archive, rather than in any subdirectory
python client.py src.tgz
```
