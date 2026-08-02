# the-fourth-server benchmarks

A like-for-like comparison of `the-fourth-server` (tf) against two popular HTTP
frameworks (**axum**, **actix-web**) and another async TCP framework
(**lynn_tcp**) on a uniform **echo** workload.

Metrics collected per run:

- **Latency** — p50 / p99 / p99.9 / max round-trip, in microseconds (HdrHistogram).
- **Throughput** — requests per second.
- **Scaling / max clients** — behaviour as concurrent connections rise (errors,
  `ok_conns`, latency growth).
- **CPU & memory** — server-process CPU% (utime+stime over the window) and peak
  RSS (`VmHWM`), read from `/proc`.

## What is measured

Every server is a minimal **echo** endpoint that returns the request payload:

| target     | framework / transport            | crypto                  | endpoint        |
|------------|----------------------------------|-------------------------|-----------------|
| `tf_plain` | the-fourth-server, `LengthDelimitedCodec` | none           | `ECHO` handler  |
| `tf_enc`   | the-fourth-server, `Spake2Encrypted`      | SPAKE2 + AES-256-GCM | `ECHO` handler  |
| `axum`     | axum / HTTP/1.1                  | none                    | `POST /echo`    |
| `actix`    | actix-web / HTTP/1.1            | none                    | `POST /echo`    |
| `lynn`     | lynn_tcp                         | none                    | route id `1`    |

`tf` is benched **twice** — plaintext (fair vs the other plaintext servers) and
SPAKE2-encrypted (its real-world headline path) — so the cost of encryption is
visible.

### Load model

`the-fourth-server`'s client multiplexes **one in-flight request per
connection**: requests are queued onto an mpsc channel and the connection task
sends one and awaits the response before taking the next. So a single connection
is a strict sequential ping-pong, and **concurrency comes from the number of
connections**. To keep the comparison fair, the HTTP and lynn clients use the
same model: one keep-alive connection per worker, one request at a time. RPS and
"max clients" therefore scale with `--conns`.

## Layout

| file | role |
|------|------|
| `src/benches/tf_plain_server.rs` / `tf_enc_server.rs` | tf echo servers |
| `src/benches/axum_server.rs` / `actix_server.rs`      | HTTP echo servers |
| `src/benches/lynn_server.rs`                          | lynn_tcp echo server |
| `src/benches/runner.rs` (`bench_runner`)             | load generator + /proc sampler |
| `src/bench/`                                          | shared lib: stats, sampler, echo protocol |
| `benchmarks/run_all.sh`                              | orchestrates the matrix → `results.md` |

## Running

```bash
cd tf-examples

# Full matrix (builds release binaries, writes benchmarks/results.md):
./benchmarks/run_all.sh

# Quick smoke run (a couple of configs, 3s each):
QUICK=1 ./benchmarks/run_all.sh

# Tune the matrix:
DURATION=10 WARMUP=3 SMALL_CONNS="50 500 2000" ./benchmarks/run_all.sh
```

Drive a single target manually:

```bash
cargo build --release --bins
target/release/bench_runner --target tf_plain --conns 500 --payload 64 \
    --duration 10 --warmup 3
# RESULT target=tf_plain conns=500 payload=64 ... rps=... p99_us=... cpu_pct=... peak_rss_mb=...
```

`bench_runner` spawns the server itself. Pass `--no-spawn` to drive a server you
started separately (then start the matching `bench_*_server --port <p>` first).

## Methodology notes & caveats

- **Single box.** Client (runner) and server share the same 24-core machine and
  loopback, so they compete for CPU. Absolute numbers are lower than a two-box
  setup would show, but the *relative* comparison is consistent because every
  target is driven identically. For publishable absolute figures, run the server
  and runner on separate hosts (`--no-spawn`).
- **File-descriptor ceiling.** The hard `nofile` limit here is **4096**, which
  caps usable concurrency to ~3500 connections (each connection costs one fd on
  the runner *and* one on the server). "Max clients" is reported up to that
  ceiling; raise the limit (root: `ulimit -n`) to push further.
- **TCP_NODELAY.** Enabled everywhere. tf and the HTTP clients set it by default;
  lynn defaults it *off* (which adds ~40 ms of Nagle/delayed-ACK latency to a
  ping-pong), so the lynn server explicitly enables it. lynn's per-IP connection
  cap and rate limit are also lifted, since all load originates from 127.0.0.1.
- **Thread counts.** All servers use framework defaults (tokio multi-thread =
  logical cores for tf/axum/lynn; actix workers = logical cores).
- **CLK_TCK** is assumed to be 100 (the universal Linux default) when converting
  jiffies to CPU%.
- **Build profile.** Release with `opt-level=3` + thin LTO (see
  `tf-examples/Cargo.toml`). Thin (not fat) LTO is used to keep the heavy
  axum/actix/reqwest/lynn build times reasonable; the effect on hot-path codegen
  is negligible.

## Output

`run_all.sh` writes `benchmarks/results.md` (rendered tables) and
`benchmarks/results.raw` (the raw `RESULT` lines). Server stderr from each run is
appended to `benchmarks/run.log`.
