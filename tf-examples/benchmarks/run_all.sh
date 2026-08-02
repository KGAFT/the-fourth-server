#!/usr/bin/env bash
#
# Orchestrates the benchmark matrix and renders a markdown report.
#
# For every (target, connections, payload) combination it launches bench_runner,
# which spawns the server, drives it, samples CPU/RSS and prints a RESULT line.
# Those lines are collected and turned into benchmarks/results.md.
#
# Override knobs via env, e.g.:  DURATION=10 WARMUP=3 ./benchmarks/run_all.sh
# Quick smoke run:               QUICK=1 ./benchmarks/run_all.sh
set -u

cd "$(dirname "$0")/.." || exit 1   # -> tf-examples/

# This box's hard nofile limit caps concurrent connections (~4096). Raise the
# soft limit as high as the hard limit allows.
ulimit -n "$(ulimit -Hn)" 2>/dev/null || true

# Always leave the box clean, even if interrupted: orphaned server children
# from a killed run would otherwise corrupt later runs (stale port owners).
cleanup() { pkill -f 'bench_.*_server' 2>/dev/null; }
trap cleanup EXIT INT TERM
cleanup
sleep 1

DURATION=${DURATION:-8}
WARMUP=${WARMUP:-2}
RUNNER=target/release/bench_runner
RAW=benchmarks/results.raw
OUT=benchmarks/results.md
RUNLOG=benchmarks/run.log

TARGETS=${TARGETS:-"tf_plain tf_enc axum actix lynn"}
SMALL_PAYLOAD=64
LARGE_PAYLOAD=65536
if [ "${QUICK:-0}" = "1" ]; then
  SMALL_CONNS=${SMALL_CONNS:-"1 200"}
  LARGE_CONNS=${LARGE_CONNS:-"200"}
  DURATION=3; WARMUP=1
else
  SMALL_CONNS=${SMALL_CONNS:-"1 50 200 1000 2000 3500"}
  LARGE_CONNS=${LARGE_CONNS:-"50 200"}
fi

echo "Building release binaries..." >&2
cargo build --release --bins 2>&1 | tail -2 || exit 1

: > "$RAW"
: > "$RUNLOG"

run() {
  local target=$1 conns=$2 payload=$3
  # Ensure no stale server owns the port before we start.
  cleanup; sleep 0.3
  echo ">>> $target conns=$conns payload=${payload}B" >&2
  local line
  line=$(timeout 180 "$RUNNER" --target "$target" --conns "$conns" \
            --payload "$payload" --duration "$DURATION" --warmup "$WARMUP" \
            2>>"$RUNLOG" | grep '^RESULT')
  if [ -n "$line" ]; then
    echo "$line" | tee -a "$RAW" >&2
  else
    echo "  (no result - server failed or timed out, see $RUNLOG)" >&2
    echo "RESULT target=$target conns=$conns payload=$payload dur=$DURATION ok_conns=0 rps=0 p50_us=0 p99_us=0 p999_us=0 max_us=0 count=0 errors=-1 cpu_pct=0 peak_rss_mb=0" >> "$RAW"
  fi
  # Make sure no server child outlives the run (e.g. on runner timeout).
  pkill -f 'bench_.*_server' 2>/dev/null
  sleep 1
}

for c in $SMALL_CONNS; do
  for t in $TARGETS; do run "$t" "$c" "$SMALL_PAYLOAD"; done
done
for c in $LARGE_CONNS; do
  for t in $TARGETS; do run "$t" "$c" "$LARGE_PAYLOAD"; done
done

# ---- Render markdown report -------------------------------------------------
render_report() {
  {
    echo "# Benchmark results"
    echo
    echo "- Date: $(date -u '+%Y-%m-%d %H:%M UTC')"
    echo "- Host: $(uname -srm), $(nproc) logical cores, $(free -h | awk '/Mem:/{print $2}') RAM"
    echo "- nofile (fd) limit: $(ulimit -n) — caps max concurrent connections on this box"
    echo "- Measurement window: ${DURATION}s after ${WARMUP}s warmup"
    echo "- Workload: echo. Each connection is a strict sequential request/response loop; concurrency = connection count."
    echo "- CPU%: server process utime+stime over the window (>100% means multiple cores). RSS: peak (VmHWM)."
    echo
    for payload in "$SMALL_PAYLOAD" "$LARGE_PAYLOAD"; do
      echo "## Payload ${payload} B"
      echo
      echo "| target | conns | rps | p50 µs | p99 µs | p99.9 µs | max µs | cpu % | peak RSS MB | ok/conns | errors |"
      echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
      awk -v p="$payload" '
        /^RESULT/{
          delete v; for(i=2;i<=NF;i++){split($i,a,"="); v[a[1]]=a[2]}
          if(v["payload"]==p)
            printf "| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s/%s | %s |\n",
              v["target"],v["conns"],v["rps"],v["p50_us"],v["p99_us"],v["p999_us"],
              v["max_us"],v["cpu_pct"],v["peak_rss_mb"],v["ok_conns"],v["conns"],v["errors"]
        }' "$RAW"
      echo
    done
  } > "$OUT"
}
render_report

echo >&2
echo "Wrote $OUT" >&2
cat "$OUT" >&2
