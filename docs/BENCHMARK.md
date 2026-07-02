# BENCHMARK (M0)

## Environment

- Windows 11 Home 10.0.26200, WebView2 runtime
- Rust 1.96.0 (debug profile for both harnesses), Node 22.14.0, Vite 6.4.3
- Shell under test: pwsh (PowerShell 7)
- Date: 2026-07-02

Debug-profile numbers are conservative; release builds can only be faster.

## Harness 1: in-app end-to-end autotest

Command:

```powershell
$env:TERMF_AUTOTEST='1'; $env:TERMF_REPORT_PATH="$PWD\autotest-report.json"
npx tauri dev
```

Drives the real UI (WebView2 + xterm.js + Tauri IPC + ConPTY): splits panes,
creates/switches/deletes workspaces, checks PTY echo and keep-alive, measures
user-visible workspace switch latency (snapshot + dispose + switch command +
render + mount + snapshot restore + replay + focus), then a ~46 s app-level
soak with a pane continuously producing output. Raw report:
`autotest-report.json`.

### Results (2026-07-02)

| Check | Result |
|---|---|
| pane split creates pane | PASS |
| PTY echo I/O (`echo TERMF_ECHO_OK`) | PASS |
| keep-alive across workspace switch | PASS (tick 7 → 34 while inactive) |
| workspace create/delete | PASS |

| Metric | Value | M0 target |
|---|---|---|
| workspace switch p50 | 67.0 ms | — |
| **workspace switch p95** (30 samples) | **78.3 ms** | **< 150 ms ✓** |
| workspace switch max | 81.9 ms | — |
| soak 46 s, backend RSS growth | ×1.031 (33.8 → 34.8 MB) | — (see harness 2 for the 10-min soak) |

## Harness 2: headless backend bench (K=2 × N=2, 10-min soak)

Command:

```powershell
cd src-tauri
$env:TERMF_BENCH_OUT='..\bench-report.json'
cargo run --bin bench -- --soak-secs 600
```

Backend process only (no WebView2). 4 live PTY sessions; all panes produce
output continuously during the soak (one line / 50 ms each); the emitter pump
runs at the app's 16 ms cadence; the active workspace alternates every 5 s
with replay. RSS via sysinfo, sampled every 30 s. Raw report:
`bench-report.json`.

### Results (2026-07-02)

| Metric | Value | M0 target |
|---|---|---|
| spawn 4 PTYs (2 ws × 2 panes) | 78.7 ms total | — |
| output throughput into ring buffer | 2.88 MiB/s (2.35 MB in 0.78 s, ConPTY-paced) | — |
| backend-side switch + replay p50 / p95 (100 iters) | 0.012 ms / 0.013 ms | — (UI dominates; see harness 1) |
| RSS after spawn | 11.5 MB | — |
| **10-min soak RSS growth** | **×1.124** (12.9 → 14.5 MB) | **< 1.5× ✓** |
| soak RSS samples | plateau 13.8–14.6 MB (see `bench-report.json`) | — |
| emitted event batches during soak | 36,374 (~60/s across 4 flooding panes) | bounded ✓ |
| ring overflow during soak | 88,857 chunks / 4.1 MB dropped (by design, counted) | bounded memory ✓ |

Interpretation: the bounded ring buffers held backend memory flat for 10
minutes while all four panes flooded continuously — overflow was absorbed by
counted oldest-chunk drops (ADR-004), not memory growth. Backend share of a
workspace switch is negligible (~13 µs); the measured 78 ms end-to-end p95
is xterm mount/restore + render (harness 1).

### Phase A re-verification (2026-07-02, sidebar + activity polling added)

Same autotest after the Phase A UI overhaul (sidebar, 1 s activity polling,
themes, palette). Note: this run mounted 3–4 panes per workspace (config
accumulated panes from prior runs), so it is a heavier switch than the M0
row above.

| Metric | Value | Target |
|---|---|---|
| all functional checks (echo/keep-alive/split/CRUD) | PASS | — |
| workspace switch p50 / p95 / max | 93.8 / 111.5 / 113.3 ms | p95 < 150 ms ✓ |
| soak 46 s backend RSS growth | ×1.039 | — |

## Not measured in M0 (do not claim)

- M1 targets (16 live PTYs, RSS < 1 GB incl. WebView2, cached switch p95
  < 50 ms, 60-min soak): not attempted.
- Total app RSS including WebView2: not instrumented (backend RSS only).
- Flood-output rendering smoothness: subjective metric, not automated.
