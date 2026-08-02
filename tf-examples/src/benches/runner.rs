//! Benchmark load generator.
//!
//! Spawns the chosen echo server as a child process, waits until it accepts
//! connections, then drives it with `--conns` concurrent connections for
//! `--duration` seconds (after `--warmup` seconds). Each connection issues a
//! strictly sequential request/response loop (this matches the tf client model,
//! where one connection = one in-flight request), so concurrency comes from the
//! number of connections. CPU% and peak RSS of the server are sampled from
//! /proc over the measurement window only.
//!
//! Prints one machine-readable `RESULT ...` line to stdout and a human summary
//! to stderr.
//!
//! Usage:
//!   bench_runner --target tf_plain --conns 256 --payload 64 --duration 10 --warmup 3
//!   bench_runner --target axum --conns 1000 --payload 1024 --duration 10 --no-spawn

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tf_examples::bench::stats::{BenchResult, LatencyRecorder};
use tf_examples::bench::stype::{EchoMsg, EchoSType};
use tf_examples::bench::{ECHO_METHOD_ID, default_port, proc::CpuMemSampler, server_bin, tf, util};

use tfserver::client::{ClientConnect, ClientMode, ClientRequest, DataRequest, HandlerInfo};
use tfserver::codec::codec_trait::TfCodec;
use tfserver::structures::s_type;

use tokio::process::{Child, Command};
use tfserver::tokio_util::bytes::Bytes;

/// Upper bound on a single round-trip. Generous enough not to affect healthy
/// latency, but prevents a stuck call from hanging the whole run past the
/// measurement deadline.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() {
    let target = util::arg("--target").unwrap_or_else(|| {
        eprintln!("--target required (tf_plain|tf_enc|axum|actix|lynn)");
        std::process::exit(2);
    });
    let conns: usize = util::arg_or("--conns", 256);
    let payload: usize = util::arg_or("--payload", 64);
    let duration_s: f64 = util::arg_or("--duration", 10.0);
    let warmup_s: f64 = util::arg_or("--warmup", 3.0);
    let port: u16 = util::arg_or("--port", default_port(&target));
    let spawn_server = util::arg("--no-spawn").is_none();

    // 1. Start the server child (unless told to use an externally running one).
    let mut child: Option<Child> = None;
    if spawn_server {
        let bin = server_bin(&target).unwrap_or_else(|| {
            eprintln!("unknown target: {target}");
            std::process::exit(2);
        });
        let path = util::exe_dir().join(bin);
        let c = Command::new(&path)
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|e| {
                eprintln!("failed to spawn {}: {e}", path.display());
                std::process::exit(1);
            });
        child = Some(c);
    }

    let addr = format!("127.0.0.1:{port}");
    if !util::wait_for_port(&addr, Duration::from_secs(30)).await {
        eprintln!("server {target} did not become ready on {addr}");
        kill(child).await;
        std::process::exit(1);
    }
    // Small settle so all worker threads are up before warmup.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let pid = child.as_ref().and_then(|c| c.id());

    // 2. Run the load.
    let outcome = run_load(
        &target,
        &addr,
        conns,
        payload,
        duration_s,
        warmup_s,
        pid,
    )
    .await;

    // 3. Report.
    let rec = outcome.rec;
    let result = BenchResult {
        target: target.clone(),
        conns,
        payload,
        duration_s,
        ok_conns: outcome.ok_conns,
        rps: rec.count as f64 / duration_s,
        p50_us: rec.hist.value_at_quantile(0.50),
        p99_us: rec.hist.value_at_quantile(0.99),
        p999_us: rec.hist.value_at_quantile(0.999),
        max_us: rec.hist.max(),
        count: rec.count,
        errors: rec.errors,
        cpu_pct: outcome.cpu_pct,
        peak_rss_mb: outcome.peak_rss_mb,
    };
    eprintln!("{}", result.summary());
    println!("{}", result.result_line());

    kill(child).await;
}

struct RunOutcome {
    rec: LatencyRecorder,
    ok_conns: usize,
    cpu_pct: f64,
    peak_rss_mb: f64,
}

async fn run_load(
    target: &str,
    addr: &str,
    conns: usize,
    payload: usize,
    duration_s: f64,
    warmup_s: f64,
    pid: Option<u32>,
) -> RunOutcome {
    let start = Instant::now();
    let warmup_deadline = start + Duration::from_secs_f64(warmup_s);
    let measure_deadline = warmup_deadline + Duration::from_secs_f64(duration_s);
    let ok = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(conns);
    for _ in 0..conns {
        let target = target.to_string();
        let addr = addr.to_string();
        let ok = ok.clone();
        handles.push(tokio::spawn(async move {
            run_connection(&target, &addr, payload, warmup_deadline, measure_deadline, ok).await
        }));
    }

    // Sample CPU/RSS only over the measurement window.
    sleep_until(warmup_deadline).await;
    let sampler = pid.map(CpuMemSampler::start);
    sleep_until(measure_deadline).await;
    let (cpu_pct, peak_rss_mb) = sampler.map(|s| s.finish()).unwrap_or((0.0, 0.0));

    let mut rec = LatencyRecorder::new();
    for h in handles {
        if let Ok(r) = h.await {
            rec.merge(&r);
        }
    }

    RunOutcome {
        rec,
        ok_conns: ok.load(Ordering::Relaxed),
        cpu_pct,
        peak_rss_mb,
    }
}

async fn sleep_until(deadline: Instant) {
    tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
}

async fn run_connection(
    target: &str,
    addr: &str,
    payload: usize,
    warmup_deadline: Instant,
    measure_deadline: Instant,
    ok: Arc<AtomicUsize>,
) -> LatencyRecorder {
    match target {
        "tf_plain" => {
            tf_conn(addr, payload, warmup_deadline, measure_deadline, ok, tf::plain_client_codec).await
        }
        "tf_enc" => {
            tf_conn(addr, payload, warmup_deadline, measure_deadline, ok, tf::enc_client_codec).await
        }
        "axum" | "actix" => {
            http_conn(addr, payload, warmup_deadline, measure_deadline, ok).await
        }
        "lynn" => lynn_conn(addr, payload, warmup_deadline, measure_deadline, ok).await,
        _ => LatencyRecorder::new(),
    }
}

/// One tf connection (plaintext or encrypted, selected by the codec factory).
async fn tf_conn<C: TfCodec>(
    addr: &str,
    payload: usize,
    warmup_deadline: Instant,
    measure_deadline: Instant,
    ok: Arc<AtomicUsize>,
    make_codec: impl Fn() -> C,
) -> LatencyRecorder {
    let mut rec = LatencyRecorder::new();
    let client = match ClientConnect::new(
        "localhost".to_string(),
        addr.to_string(),
        None,
        make_codec(),
        ClientMode::Tcp { client_config: None },
        8,
    )
    .await
    {
        Ok(c) => c,
        Err(_) => {
            rec.error();
            return rec;
        }
    };

    let msg = EchoMsg {
        s_type: EchoSType::Echo,
        data: vec![0u8; payload],
    };
    let data = s_type::to_bytes(&msg).expect("serialize echo msg");
    let mut counted = false;

    loop {
        let now = Instant::now();
        if now >= measure_deadline {
            break;
        }
        let measuring = now >= warmup_deadline;
        let t = Instant::now();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let req = ClientRequest {
            req: DataRequest {
                handler_info: HandlerInfo::new_named(tf::ECHO_HANDLER.to_string()),
                data: Bytes::from_owner(data.clone()),
                s_type: Box::new(EchoSType::Echo),
            },
            consumer: tx,
        };
        if client.dispatch_request(req).await.is_err() {
            rec.error();
            break;
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(_)) => {
                if !counted {
                    ok.fetch_add(1, Ordering::Relaxed);
                    counted = true;
                }
                if measuring {
                    rec.record_us(t.elapsed().as_micros() as u64);
                }
            }
            _ => {
                rec.error();
                break;
            }
        }
    }
    rec
}

/// One HTTP keep-alive connection (axum/actix). reqwest with a single pooled
/// connection per task gives strict one-request-at-a-time behaviour.
async fn http_conn(
    addr: &str,
    payload: usize,
    warmup_deadline: Instant,
    measure_deadline: Instant,
    ok: Arc<AtomicUsize>,
) -> LatencyRecorder {
    let mut rec = LatencyRecorder::new();
    let url = format!("http://{addr}/echo");
    let client = match reqwest::Client::builder()
        .http1_only()
        .pool_max_idle_per_host(1)
        .tcp_nodelay(true)
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            rec.error();
            return rec;
        }
    };
    let body = vec![0u8; payload];
    let mut counted = false;

    loop {
        let now = Instant::now();
        if now >= measure_deadline {
            break;
        }
        let measuring = now >= warmup_deadline;
        let t = Instant::now();
        let outcome = tokio::time::timeout(REQUEST_TIMEOUT, async {
            client.post(&url).body(body.clone()).send().await?.bytes().await
        })
        .await;
        match outcome {
            Ok(Ok(_)) => {
                if !counted {
                    ok.fetch_add(1, Ordering::Relaxed);
                    counted = true;
                }
                if measuring {
                    rec.record_us(t.elapsed().as_micros() as u64);
                }
            }
            _ => {
                rec.error();
                break;
            }
        }
    }
    rec
}

/// One lynn_tcp connection.
async fn lynn_conn(
    addr: &str,
    payload: usize,
    warmup_deadline: Instant,
    measure_deadline: Instant,
    ok: Arc<AtomicUsize>,
) -> LatencyRecorder {
    use lynn_tcp::{lynn_client::*, lynn_tcp_dependents::*};

    let mut rec = LatencyRecorder::new();
    let mut client = LynnClient::new_with_addr(addr).await.start().await;
    let payload_bytes = bytes::Bytes::from(vec![0u8; payload]);
    let mut counted = false;

    loop {
        let now = Instant::now();
        if now >= measure_deadline {
            break;
        }
        let measuring = now >= warmup_deadline;
        let t = Instant::now();
        let outcome = tokio::time::timeout(REQUEST_TIMEOUT, async {
            client
                .send_data(HandlerResult::new_with_send_to_server(
                    ECHO_METHOD_ID,
                    payload_bytes.clone(),
                ))
                .await
                .map_err(|_| ())?;
            client.get_receive_data().await.ok_or(())
        })
        .await;
        match outcome {
            Ok(Ok(_)) => {
                if !counted {
                    ok.fetch_add(1, Ordering::Relaxed);
                    counted = true;
                }
                if measuring {
                    rec.record_us(t.elapsed().as_micros() as u64);
                }
            }
            _ => {
                rec.error();
                break;
            }
        }
    }
    rec
}

async fn kill(child: Option<Child>) {
    if let Some(mut c) = child {
        let _ = c.kill().await;
        let _ = c.wait().await;
    }
}