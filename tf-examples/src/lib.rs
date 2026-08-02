//! Shared helpers for the benchmark binaries (`bench_*`).
//!
//! The benchmark suite compares `the-fourth-server` against axum, actix-web and
//! lynn_tcp on a uniform echo workload. See `benchmarks/README.md`.
pub mod bench;