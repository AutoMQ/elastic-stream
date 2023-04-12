## Introduction
ElasticStream is an open-source, elastic streaming platform. ElasticStream is designed to stand on shoulders of cloud infra-structure,
excelling itself in scalability, fault-tolerance, cost-effectiveness and security.

ElasticStream has three components: placement-manager, data-node and frontend.

## How to Build


## Collect and Report Unit Test Coverage

```sh
cargo llvm-cov
```

With HTML report,
```sh
cargo llvm-cov --html
```
or 
```sh
cargo llvm-cov --open
```

If wishing to execute `cargo run` instead of `cargo test`, use run sub-command:

```sh
cargo llvm-cov run
```

Read [more](https://crates.io/crates/cargo-llvm-cov)

Sample [integration](https://github.com/taiki-e/cargo-llvm-cov) with github action.

## How to Contribute

## Notes

### **communicating-between-sync-and-async-code**
`Store` module is built on top of io-uring directly. The `Server` module, however, is built using `tokio-uring`, following thread-per-core paradigm, which as a result is fully async. Reading and writing records between these two modules involve communication between async and sync code, as shall comply with [the following guideline](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html#communicating-between-sync-and-async-code) 

## Run with Address Sanitizer 

Sometimes you have to deal with low-level operations, for example, interacting with DMA requires page alignment memory. Unsafe code is required to handle these cases and address sanitizer would be helpful to maintain memory safety.

```sh
RUSTFLAGS=-Zsanitizer=address cargo test test_layout -Zbuild-std --target x86_64-unknown-linux-gnu
```
Read the [following link](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html) for more advanced usage.


## Run

### Launch Data-Node Server
```sh
cargo run --bin data-node -- -c etc/config.yaml
```