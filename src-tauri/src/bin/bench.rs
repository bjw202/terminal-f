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

    // ---- 5. SPEC-PTY-FLOW-001 M3: flow-control soak (AC-11) ---------------
    // @MX:NOTE: headless bench 에서 흐름 제어(ack-watermark)를 관측한다.
    // plan §B.5: headless bench 는 프론트엔드가 없어 ack=0 → outstanding 이
    // 즉시 HIGH(128 KiB) 도달해 emitter 가 정지(R3)한다. Phase A(ack 없음)에서
    // emitter 정지 + outstanding 증가를 관측하고, Phase B(ack 합성)에서 ack 가
    // outstanding 을 drain 하여 emitter 가 재개되고 ack 흐름 중 oldest-drop 가
    // 없음(R14)을 확인한다.
    //
    // @MX:NOTE: reader park(R4) 관측 제약 — plan §B.5 는 bench/test 빌드에서
    // 축소 워터마크 주입을 요구하나, spawn_session 에 config 주입 경로가 없다
    // (FlowState::with_config 는 standalone 인스턴스에만 사용 가능, spawned 세션은
    // Arc<PtySession> 배후라 config 필드 수정 불가). SPEC-PTY-FLOW-002 M2 부터
    // stall_timeout 만 예외적으로 env 오버라이드 가능하다 — FlowConfig::default 가
    // TERMF_FLOW_STALL_TIMEOUT_MS 를 1회 파싱하며, bench 는 Phase A 의 정확히 10초
    // 무ack 구간과 밸브 기본 stall_timeout(10s)의 경계 충돌(B10)을 피하려고
    // 60000(ms)으로 확대해 실행한다(AC-13). 나머지 워터마크는 여전히 주입 불가.
    // 또한 M1 의
    // check_reader_park_gate 가 un_emitted 를 chunk 수(last_seq 차이)로 산출하여
    // byte 단위 RING_PAUSE_THRESHOLD(768 KiB=786432)와 단위가 불일치해 기본
    // config 에서 reader park 가 사실상 발생하지 않는다. 두 M1 갭 모두 본 M3
    // 범위(PRESERVE) 밖이므로, bench 는 emitter 레벨(R3/R2/R1) 관측에 한정하며
    // reader park/밸브(R4/R6)는 flow_tests.rs 단위 테스트로 검증된 상태로 둔다.
    println!("[bench] flow-control soak (SPEC-PTY-FLOW-001 AC-11) ...");

    // 기존 soak 명령이 실행 중인 팬을 재사용하면 Ctrl+C 로 중단이 불확실해
    // (pwsh Start-Sleep 파이프라인이 간헐적으로 interrupt 를 삼킴), 전용 fresh
    // 팬을 추가 spawn 하여 흐름 제어 관측의 간섭을 원천 차단한다.
    let flow_pane = "bench-flow-pane";
    registry
        .spawn_session("bench-ws-0", flow_pane, &home, None)
        .expect("spawn flow pane failed");
    // 프롬프트 안정화 대기 + DSR 펌핑.
    let flow_prompt_deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < flow_prompt_deadline {
        pump_dsr(&registry, flow_pane, dsr.entry(flow_pane.to_string()).or_insert(0));
        let ready = registry
            .session_for_pane(flow_pane)
            .map(|s| s.ring.lock().unwrap().total_bytes > 20)
            .unwrap_or(false);
        if ready {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // live 모드 진입 — emitter 가 방출하도록 replay_synced=true 설정.
    registry.set_active_workspace(Some("bench-ws-0"));
    let _ = registry.replay(flow_pane, 0);
    std::thread::sleep(Duration::from_millis(500));

    // 빠른 홍수 — throughput 테스트와 동일 패턴(검증됨). outstanding 이
    // HIGH(128 KiB)를 초과해 emitter 가 정지(R3)하게 만든다.
    let _ = registry.write_pane(
        flow_pane,
        "$s='x'*8190; 1..20000 | ForEach-Object { $s }; 'TERMF_FLOW_DONE'\r",
    );

    // Phase A: ack 없음 → emitter 정지(R3) + outstanding 증가 관측.
    let mut flow_samples: Vec<serde_json::Value> = Vec::new();
    let mut saw_emitter_paused = false;
    let phase_a_start = Instant::now();
    let phase_a_deadline = phase_a_start + Duration::from_secs(10);
    while Instant::now() < phase_a_deadline {
        let _ = output::pump_once(&registry, &|_| {}, &|_| {});
        pump_dsr(&registry, flow_pane, dsr.entry(flow_pane.to_string()).or_insert(0));
        if let Some(s) = registry.flow_stats(flow_pane) {
            if s.emitter_paused {
                saw_emitter_paused = true;
            }
            flow_samples.push(serde_json::json!({
                "phase": "A",
                "t_ms": phase_a_start.elapsed().as_millis() as u64,
                "emitted": s.emitted,
                "acked": s.acked,
                "outstanding": s.outstanding,
                "emitter_paused": s.emitter_paused,
                "reader_parked": s.reader_parked,
                // SPEC-PTY-FLOW-002 AC-13 — 손수 나열하는 표본이므로 신규 필드가
                // 구조체에서 자동으로 따라오지 않는다. 명시적으로 열거한다.
                "emitter_valve_fired": s.emitter_valve_fired,
            }));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let outstanding_at_end_a = registry
        .session_for_pane(flow_pane)
        .map(|s| s.flow_state.flow_stats().outstanding)
        .unwrap_or(0);

    // Phase B: ack 합성 → emitter 재개, outstanding drain, oldest-drop 없음 관측.
    let ring_drop_before_b = registry
        .session_for_pane(flow_pane)
        .map(|s| {
            let r = s.ring.lock().unwrap();
            (r.dropped_chunks, r.dropped_bytes, r.total_bytes)
        })
        .unwrap_or((0, 0, 0));
    // @MX:NOTE: LOW(32 KiB) 기준 — outstanding 을 LOW 근처로 drain 하는 ack 를
    // 합성한다(emitter 재개 유도). 기본 config 의 LOW 워터마크 사용.
    const FLOW_LOW_WATERMARK: usize = 32 * 1024;
    let phase_b_start = Instant::now();
    let phase_b_deadline = phase_b_start + Duration::from_secs(6);
    while Instant::now() < phase_b_deadline {
        let _ = output::pump_once(&registry, &|_| {}, &|_| {});
        pump_dsr(&registry, flow_pane, dsr.entry(flow_pane.to_string()).or_insert(0));
        // ack 합성: outstanding > LOW 면 그 차이만큼 ack → emitter 재개 → ring drain.
        if let Some(s) = registry.flow_stats(flow_pane) {
            if s.outstanding > FLOW_LOW_WATERMARK {
                let ack = s.outstanding - FLOW_LOW_WATERMARK;
                let _ = registry.ack_output(flow_pane, ack as u64);
            }
            flow_samples.push(serde_json::json!({
                "phase": "B",
                "t_ms": phase_b_start.elapsed().as_millis() as u64,
                "emitted": s.emitted,
                "acked": s.acked,
                "outstanding": s.outstanding,
                "emitter_paused": s.emitter_paused,
                "reader_parked": s.reader_parked,
                // SPEC-PTY-FLOW-002 AC-13 — Phase A 와 동일하게 명시 열거.
                "emitter_valve_fired": s.emitter_valve_fired,
            }));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let ring_drop_after_b = registry
        .session_for_pane(flow_pane)
        .map(|s| {
            let r = s.ring.lock().unwrap();
            (r.dropped_chunks, r.dropped_bytes, r.total_bytes)
        })
        .unwrap_or((0, 0, 0));
    let oldest_drop_during_ack = ring_drop_after_b.0.saturating_sub(ring_drop_before_b.0);
    // Phase B 중 outstanding 이 LOW 이하로 떨어진 적이 있는지(ack 가 emitter 를
    // 재개시켰는지) 확인.
    let saw_outstanding_drained_in_b = flow_samples
        .iter()
        .filter_map(|v| {
            let phase = v.get("phase").and_then(|p| p.as_str())?;
            let outstanding = v.get("outstanding")?.as_u64()?;
            (phase == "B").then_some(outstanding)
        })
        .any(|o| o <= FLOW_LOW_WATERMARK as u64);

    println!(
        "[bench] flow soak done: emitterPausedInA:{saw_emitter_paused} outstandingEndA:{outstanding_at_end_a} outstandingDrainedInB:{saw_outstanding_drained_in_b} oldest_drop_during_ack:{oldest_drop_during_ack}"
    );

    // AC-11 판정: emitter 게이트(R3) 작동 + ack 합성(R2)으로 outstanding drain +
    // ack 흐름 중 oldest-drop 없음. reader park(R4)는 위 M1 갭으로 본 bench 에서
    // 관측 불가 — 단위 테스트(flow_tests.rs AC-3/4/5)로 검증됨.
    let flow_ok = saw_emitter_paused
        && saw_outstanding_drained_in_b
        && oldest_drop_during_ack == 0;
    report.insert("flow_saw_emitter_paused".into(), saw_emitter_paused.into());
    report.insert("flow_outstanding_drained_in_b".into(), saw_outstanding_drained_in_b.into());
    report.insert("flow_oldest_drop_during_ack".into(), oldest_drop_during_ack.into());
    report.insert("flow_ok".into(), flow_ok.into());
    report.insert("flow_samples".into(), flow_samples.into());

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
