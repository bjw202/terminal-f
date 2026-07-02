//! M0 benchmark harness (backend-level, headless — no WebView).
//!
//! Measures, for K=2 workspaces x N=2 panes:
//!   - PTY spawn latency
//!   - simple output throughput (bytes/sec into the ring buffer)
//!   - registry-level workspace switch + replay latency (p50/p95)
//!   - soak: RSS growth while all panes continuously produce output
//!
//! Note: this measures the backend process only. End-to-end UI switch latency
//! (including xterm mount/restore) is measured separately by the in-app
//! autotest (TERMF_AUTOTEST=1); WebView2 RSS is not included here.
//!
//! Usage: bench [--soak-secs N]   (default 60)

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminal_f_lib::output;
use terminal_f_lib::session::SessionRegistry;

const K_WORKSPACES: usize = 2;
const N_PANES: usize = 2;

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

/// Emulate the terminal side of the DSR (`ESC[6n`) handshake that pwsh's
/// startup performs; xterm.js does this automatically in the real app.
fn pump_dsr(registry: &SessionRegistry, pane: &str, responded: &mut usize) {
    let text = registry
        .session_for_pane(pane)
        .map(|s| s.ring.lock().unwrap().collect_since(0).0)
        .unwrap_or_default();
    let asks = text.matches("\x1b[6n").count();
    while *responded < asks {
        let _ = registry.write_pane(pane, "\x1b[1;1R");
        *responded += 1;
    }
}

fn total_ring_bytes(registry: &SessionRegistry, panes: &[(String, String)]) -> u64 {
    panes
        .iter()
        .filter_map(|(_, pane)| registry.session_for_pane(pane))
        .map(|s| s.ring.lock().unwrap().total_bytes)
        .sum()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let soak_secs: u64 = args
        .iter()
        .position(|a| a == "--soak-secs")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let registry = Arc::new(SessionRegistry::new());
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    let mut report = serde_json::Map::new();
    report.insert("k_workspaces".into(), K_WORKSPACES.into());
    report.insert("n_panes_per_workspace".into(), N_PANES.into());
    report.insert(
        "shell".into(),
        terminal_f_lib::session::detect_shell().unwrap_or_default().into(),
    );

    // ---- 1. spawn K x N ----------------------------------------------------
    let mut panes: Vec<(String, String)> = Vec::new(); // (workspace, pane)
    let t0 = Instant::now();
    for w in 0..K_WORKSPACES {
        let ws = format!("bench-ws-{w}");
        for p in 0..N_PANES {
            let pane = format!("{ws}-pane-{p}");
            registry
                .spawn_session(&ws, &pane, &home, None)
                .expect("spawn failed");
            panes.push((ws.clone(), pane));
        }
    }
    let spawn_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("[bench] spawned {} PTYs in {spawn_ms:.1}ms", panes.len());
    report.insert("spawn_total_ms".into(), spawn_ms.into());

    // wait for shell prompts (first output on every pane), answering DSR
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut dsr: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (_, pane) in &panes {
        loop {
            pump_dsr(&registry, pane, dsr.entry(pane.clone()).or_insert(0));
            let ready = registry
                .session_for_pane(pane)
                .map(|s| s.ring.lock().unwrap().total_bytes > 20)
                .unwrap_or(false);
            if ready {
                break;
            }
            if Instant::now() > deadline {
                eprintln!("[bench] WARN: pane {pane} produced no output within 30s");
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    println!("[bench] all shells ready");
    let rss_start = terminal_f_lib::current_rss_bytes();
    report.insert("rss_after_spawn_bytes".into(), rss_start.into());

    // ---- 2. throughput ------------------------------------------------------
    // ~2 MB of output on one pane, measure ring ingestion rate.
    let (_, pane0) = &panes[0];
    let before_bytes = total_ring_bytes(&registry, &panes[..1].to_vec());
    let t = Instant::now();
    registry
        .write_pane(
            pane0,
            "$s='x'*8190; 1..256 | ForEach-Object { $s }; 'TERMF_BENCH_DONE'\r",
        )
        .expect("write failed");
    let mut done = false;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        pump_dsr(&registry, pane0, dsr.entry(pane0.clone()).or_insert(0));
        let sess = registry.session_for_pane(pane0).unwrap();
        let (tail, _, _) = {
            let ring = sess.ring.lock().unwrap();
            let last = ring.last_seq();
            ring.collect_since(last.saturating_sub(5))
        };
        if tail.contains("TERMF_BENCH_DONE")
            && sess.ring.lock().unwrap().total_bytes - before_bytes > 2_000_000
        {
            done = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let secs = t.elapsed().as_secs_f64();
    let bytes = total_ring_bytes(&registry, &panes[..1].to_vec()) - before_bytes;
    let mbps = bytes as f64 / 1_048_576.0 / secs;
    println!(
        "[bench] throughput: {bytes} bytes in {secs:.2}s = {mbps:.2} MiB/s (marker seen: {done})"
    );
    report.insert("throughput_bytes".into(), bytes.into());
    report.insert("throughput_secs".into(), secs.into());
    report.insert("throughput_mib_per_sec".into(), mbps.into());
    report.insert("throughput_marker_seen".into(), done.into());

    // ---- 3. workspace switch latency (registry level) ----------------------
    // set_active + replay of every pane in the target workspace; this is the
    // backend share of a workspace switch (excludes xterm mount/render).
    let mut latencies: Vec<f64> = Vec::new();
    let mut last_seq: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for i in 0..100 {
        let ws_idx = i % K_WORKSPACES;
        let ws = format!("bench-ws-{ws_idx}");
        let t = Instant::now();
        registry.set_active_workspace(Some(&ws));
        for (pw, pane) in &panes {
            if pw == &ws {
                let from = *last_seq.get(pane).unwrap_or(&0);
                let r = registry.replay(pane, from).expect("replay failed");
                last_seq.insert(pane.clone(), r.last_seq);
            }
        }
        latencies.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = percentile(&latencies, 0.5);
    let p95 = percentile(&latencies, 0.95);
    println!("[bench] backend switch+replay: p50={p50:.2}ms p95={p95:.2}ms");
    report.insert("backend_switch_p50_ms".into(), p50.into());
    report.insert("backend_switch_p95_ms".into(), p95.into());

    // ---- 4. soak ------------------------------------------------------------
    // All panes produce output continuously; emitter pump runs like the app's
    // 16ms loop; active workspace alternates every 5s with replay.
    println!("[bench] soak for {soak_secs}s ...");
    for (_, pane) in &panes {
        registry
            .write_pane(
                pane,
                "1..100000 | ForEach-Object { \"soak tick $_ ================================\"; Start-Sleep -Milliseconds 50 }\r",
            )
            .expect("write failed");
    }
    let rss_soak_start = terminal_f_lib::current_rss_bytes();
    let soak_t0 = Instant::now();
    let mut samples: Vec<serde_json::Value> = Vec::new();
    let mut next_switch = Instant::now();
    let mut next_sample = Instant::now();
    let mut ws_flip = 0usize;
    let mut emitted_events = 0usize;
    let mut next_dsr = Instant::now();
    while soak_t0.elapsed() < Duration::from_secs(soak_secs) {
        emitted_events += output::pump_once(&registry, &|_| {}, &|_| {});
        if Instant::now() >= next_dsr {
            for (_, pane) in &panes {
                pump_dsr(&registry, pane, dsr.entry(pane.clone()).or_insert(0));
            }
            next_dsr = Instant::now() + Duration::from_millis(500);
        }
        if Instant::now() >= next_switch {
            ws_flip += 1;
            let ws = format!("bench-ws-{}", ws_flip % K_WORKSPACES);
            registry.set_active_workspace(Some(&ws));
            for (pw, pane) in &panes {
                if pw == &ws {
                    let from = *last_seq.get(pane).unwrap_or(&0);
                    if let Ok(r) = registry.replay(pane, from) {
                        last_seq.insert(pane.clone(), r.last_seq);
                    }
                }
            }
            next_switch = Instant::now() + Duration::from_secs(5);
        }
        if Instant::now() >= next_sample {
            let rss = terminal_f_lib::current_rss_bytes();
            samples.push(serde_json::json!({
                "t_secs": soak_t0.elapsed().as_secs(),
                "rss_bytes": rss,
            }));
            println!(
                "[bench] soak t={}s rss={:.1} MiB",
                soak_t0.elapsed().as_secs(),
                rss as f64 / 1_048_576.0
            );
            next_sample = Instant::now() + Duration::from_secs(30);
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let rss_soak_end = terminal_f_lib::current_rss_bytes();
    let growth = if rss_soak_start > 0 {
        rss_soak_end as f64 / rss_soak_start as f64
    } else {
        0.0
    };
    println!(
        "[bench] soak done: rss {:.1} -> {:.1} MiB (x{growth:.3}), emitted batches: {emitted_events}",
        rss_soak_start as f64 / 1_048_576.0,
        rss_soak_end as f64 / 1_048_576.0
    );
    report.insert("soak_secs".into(), soak_secs.into());
    report.insert("soak_rss_start_bytes".into(), rss_soak_start.into());
    report.insert("soak_rss_end_bytes".into(), rss_soak_end.into());
    report.insert("soak_rss_growth_factor".into(), growth.into());
    report.insert("soak_rss_samples".into(), samples.into());
    report.insert("soak_emitted_batches".into(), emitted_events.into());

    // total dropped stats
    let (dropped_chunks, dropped_bytes): (u64, u64) = panes
        .iter()
        .filter_map(|(_, p)| registry.session_for_pane(p))
        .map(|s| {
            let r = s.ring.lock().unwrap();
            (r.dropped_chunks, r.dropped_bytes)
        })
        .fold((0, 0), |acc, x| (acc.0 + x.0, acc.1 + x.1));
    report.insert("ring_dropped_chunks".into(), dropped_chunks.into());
    report.insert("ring_dropped_bytes".into(), dropped_bytes.into());

    registry.shutdown();
    println!("[bench] shutdown complete");

    // sanity: replay_synced flags don't matter post-shutdown; suppress unused warnings
    let _ = Ordering::SeqCst;

    let out_path = std::env::var("TERMF_BENCH_OUT").unwrap_or_else(|_| "bench-report.json".into());
    let json = serde_json::Value::Object(report);
    std::fs::write(&out_path, serde_json::to_string_pretty(&json).unwrap())
        .expect("failed to write bench report");
    println!("[bench] report written to {out_path}");
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
}
