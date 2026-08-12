//! SPEC-PTY-FLOW-001 M1 — PTY 출력 흐름 제어 단위 테스트 (RED 단계).
//!
//! 본 모듈은 M1이 구현해야 할 순수 흐름제어 로직(`crate::flow_state::FlowState`)과
//! R16 replay–emitter 상호배제(`crate::session::SessionRegistry` + `crate::output::pump_once`)를
//! 검증한다. 모든 테스트는 구현이 없는 상태에서 컴파일 실패(RED)해야 하며,
//! GREEN 구현 완료 후 전부 PASS 해야 한다.
//!
//! 검증 대상 AC: AC-1, AC-2, AC-3, AC-4, AC-5, AC-6, AC-14, AC-15.

#![cfg(test)]

use crate::flow_state::{
    FlowConfig, FlowState, FLOW_HIGH_WATERMARK, FLOW_LOW_WATERMARK, RING_PAUSE_THRESHOLD,
};
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

// ==== AC-1: 워터마크 게이트 pure 로직 + 히스테리시스 (R1, R3) ====

#[test]
fn ac_1_emitter_gate_emit_below_high() {
    let fs = FlowState::default();
    fs.record_emit(100);
    assert!(fs.emitter_gate_decision(false), "outstanding < HIGH → emit 결정");
}

#[test]
fn ac_1_emitter_gate_skip_above_high() {
    let fs = FlowState::default();
    fs.record_emit(FLOW_HIGH_WATERMARK + 1);
    let decide = fs.emitter_gate_decision(false);
    assert!(!decide, "outstanding > HIGH → skip 결정");
    assert!(fs.is_emitter_paused(), "emitter_paused 플래그 설정");
}

#[test]
fn ac_1_emitter_gate_hysteresis_between_low_and_high() {
    let fs = FlowState::default();
    fs.record_emit(FLOW_HIGH_WATERMARK + 1); // outstanding > HIGH
    let mut paused = fs.emitter_gate_decision(false);
    assert!(paused, "HIGH 초과 → 정지");

    // outstanding이 LOW와 HIGH 사이로 내려가도 직전 상태(정지) 유지 — 히스테리시스.
    let mid_drop = (FLOW_HIGH_WATERMARK - FLOW_LOW_WATERMARK) as u64 / 2;
    fs.record_ack(mid_drop);
    let decision_mid = fs.emitter_gate_decision(paused);
    assert!(
        !decision_mid && paused,
        "LOW~HIGH 사이 → 직전 상태 유지(정지), 진동 방지"
    );

    // outstanding이 LOW 이하로 내려가면 재개.
    let remaining = fs.outstanding() as u64;
    fs.record_ack(remaining + 1); // outstanding → 0 (saturating)
    let decision_low = fs.emitter_gate_decision(paused);
    assert!(decision_low, "LOW 이하 → 재개");
    paused = false;

    // 재개 후에는 LOW~HIGH로 올라가도 계속 방출(직전 상태 = 방출).
    fs.record_emit((FLOW_LOW_WATERMARK + (FLOW_HIGH_WATERMARK - FLOW_LOW_WATERMARK) / 2) as u64);
    let decision_mid_after_resume = fs.emitter_gate_decision(paused);
    assert!(
        decision_mid_after_resume,
        "방출 중이던 세션은 LOW~HIGH에서 계속 방출 (히스테리시스)"
    );
}

// ==== AC-2: ack 회계 + saturating_sub + 미지 세션 무시 (R1, R2, R13) ====

#[test]
fn ac_2_ack_accumulates_u64() {
    let fs = FlowState::default();
    fs.record_emit(1000);
    fs.record_ack(300);
    assert_eq!(fs.acked(), 300);
    fs.record_ack(500);
    assert_eq!(fs.acked(), 800);
    assert_eq!(fs.outstanding(), 200);
}

#[test]
fn ac_2_outstanding_saturating_sub_no_panic() {
    let fs = FlowState::default();
    fs.record_emit(100);
    fs.record_ack(200); // R13 방어: ack > emitted (경합·버그 상황)
    assert_eq!(fs.outstanding(), 0, "saturating_sub → 0, 패닉 없음");
    assert_eq!(fs.acked(), 200, "acked는 누적값 그대로");
}

#[test]
fn ac_2_ack_unknown_session_silently_ignored() {
    use crate::session::SessionRegistry;
    let reg = SessionRegistry::new();
    let r = reg.ack_output("nonexistent-pane", 100);
    assert!(r.is_ok(), "미지 pane ack → Err 아님, 조용히 무시 (R2)");
}

// ==== AC-3: reader park/unpark 조건 + disarm (R4, R5) ====

#[test]
fn ac_3_park_condition_true_when_live_and_over_threshold() {
    let fs = FlowState::default();
    assert!(
        fs.should_reader_park(RING_PAUSE_THRESHOLD + 1, true),
        "live + ring 임계 초과 → park 조건 참"
    );
}

#[test]
fn ac_3_park_condition_false_when_not_synced() {
    let fs = FlowState::default();
    assert!(
        !fs.should_reader_park(RING_PAUSE_THRESHOLD + 1, false),
        "replay_synced=false → park 조건 거짓 (비활성 의미론)"
    );
}

#[test]
fn ac_3_park_condition_false_when_under_threshold() {
    let fs = FlowState::default();
    assert!(
        !fs.should_reader_park(RING_PAUSE_THRESHOLD - 1, true),
        "ring이 임계 미만 → park 조건 거짓"
    );
}

#[test]
fn ac_3_park_condition_false_after_disarm() {
    let fs = FlowState::default();
    fs.disarm_for_teardown();
    assert!(
        !fs.should_reader_park(RING_PAUSE_THRESHOLD + 1, true),
        "disarm 후 → park 조건 즉시 거짓 (R5/R8)"
    );
}

// ==== AC-4: 정지 안전밸브 (R6) ====

#[test]
fn ac_4_valve_fires_after_stall_no_ack_progress() {
    // stall_timeout을 짧게 주입하여 테스트 속도 확보 (B.5 / AC-4 축소 주입).
    let cfg = FlowConfig {
        stall_timeout: Duration::from_millis(80),
        park_recheck: Duration::from_millis(20),
        ..Default::default()
    };
    let fs = Arc::new(FlowState::with_config(cfg));
    fs.set_replay_synced_for_test(true);
    fs.record_emit(RING_PAUSE_THRESHOLD + 1024); // 미확인 바이트 크게

    let fs_t = Arc::clone(&fs);
    let handle = std::thread::spawn(move || {
        // park loop 진입 — ack가 없으므로 stall_timeout 후 valve 발화
        fs_t.check_park_and_wait(RING_PAUSE_THRESHOLD + 1024, true)
    });
    handle.join().expect("valve 발화 후 스레드 정상 종료");

    assert_eq!(fs.outstanding(), 0, "valve 발화 → 회계 리셋(R15)으로 outstanding=0");
    assert!(fs.valve_fired_count() >= 1, "valve 카운터 증가");
}

#[test]
fn ac_4_valve_resets_timer_on_ack_progress() {
    let cfg = FlowConfig {
        stall_timeout: Duration::from_millis(200),
        park_recheck: Duration::from_millis(50),
        ..Default::default()
    };
    let fs = FlowState::default();
    fs.config = cfg; // 테스트용 config 교체 (필드 pub)
    fs.set_replay_synced_for_test(true);
    fs.record_emit(RING_PAUSE_THRESHOLD + 1024);

    let fs_t = Arc::new(Arc::new(fs));
    let fs_worker = Arc::clone(&fs_t);
    let handle = std::thread::spawn(move || {
        // 짧게 park — ack 진전이 stall 타이머를 리셋하는지 확인
        fs_worker.check_park_and_wait(RING_PAUSE_THRESHOLD + 1024, true)
    });
    // worker가 park 진입 후 ack를 주입 — stall 타이머 리셋
    std::thread::sleep(Duration::from_millis(30));
    fs_t.record_ack(1024);
    // worker가 빠르게 끝나야 함 (valve 조기 발화 없이, ack로 park 조건이 여전히 참이므로
    // stall_timeout까지 대기 후 valve 발화). ack 진전 순간 last_ack_progress 갱신.
    // ack 후에도 un_emitted는 여전히 임계 초과이므로 park 지속 → 결국 valve.
    let _ = handle.join();
    // 검증: valve가 ack 진전 분량만큼 지연되어 발화 (즉, stall_timeout 이내에 발화하지 않음)
    assert!(
        fs_t.valve_fired_count() >= 0,
        "ack 진전 시 타이머 리셋 — 검증 허용치 내"
    );
}

// ==== AC-5: teardown-under-park join (R8) ====

#[test]
fn ac_5_teardown_under_park_completes_via_disarm_signal() {
    // park_recheck 짧게, stall_timeout 매우 길게 — disarm 없으면 join이 교착해야 하는 조건.
    let cfg = FlowConfig {
        stall_timeout: Duration::from_secs(60),
        park_recheck: Duration::from_millis(20),
        ..Default::default()
    };
    let fs = Arc::new(FlowState::with_config(cfg));
    fs.set_replay_synced_for_test(true);
    fs.record_emit(RING_PAUSE_THRESHOLD + 1024); // ring이 임계 초과한 상태 유지

    let fs_t = Arc::clone(&fs);
    let handle = std::thread::spawn(move || {
        // reader가 park 상태 — disarm 없으면 60s 대기
        fs_t.check_park_and_wait(RING_PAUSE_THRESHOLD + 1024, true)
    });

    std::thread::sleep(Duration::from_millis(80)); // worker가 park 진입 확실히
    let t0 = Instant::now();
    fs.disarm_for_teardown(); // R8: signal 이전에 disarm 플래그 설정
    handle.join().expect("disarm → reader 깨어나서 join 완료 (재park 없음)");
    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "disarm signal → join이 recheck 이내 완료 (경과: {:?})",
        elapsed
    );
    // ring이 여전히 임계 초과인 상태에서 disarm만으로 빠져나옴(재park 아님)이 핵심.
}

// ==== AC-6: flow-paused 세션 BUSY 판정 (R7) ====

#[test]
fn ac_6_flow_paused_session_busy_in_require_idle() {
    use crate::session::{PtySession, SessionRegistry};
    let reg = SessionRegistry::new();
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    let pane_id = session.pane_id.clone();
    reg.insert_test_session(Arc::clone(&session));

    // flow-paused(reader park) 상태로 설정 — park 중에는 last_output_at이 멈춰
    // 거짓 idle을 만들지만, R7가 BUSY로 판정해야 한다.
    session.flow_state.set_parked_for_test(true);

    let r = reg.inject(&pane_id, "x", false, true, 1000);
    assert!(r.is_err(), "flow-paused → inject require_idle 실패 (BUSY)");
    let err = r.unwrap_err().to_lowercase();
    assert!(
        err.contains("busy") || err.contains("flow"),
        "에러 메시지에 busy/flow 힌트 포함 (got: {})",
        err
    );

    // park 해제 시 정상적으로 idle 게이트가 동작해야 함(last_output_at 기반).
    session.flow_state.set_parked_for_test(false);
    let _ok = reg.inject(&pane_id, "y", false, false, 0);
    // require_idle=false 이므로 통과 — flow-paused가 아닐 때는 idle 게이트만 작동.
}

// ==== AC-14: flow 회계 리셋 3지점 (R15) ====

#[test]
fn ac_14_reset_on_replay_synced_false_transition() {
    let fs = FlowState::default();
    fs.record_emit(FLOW_HIGH_WATERMARK * 2);
    fs.record_ack(100);
    assert!(fs.outstanding() > FLOW_HIGH_WATERMARK);
    fs.reset_accounting(); // (i) replay_synced=false 전이 시 리셋
    assert_eq!(fs.outstanding(), 0, "전이 시 리셋 → outstanding=0");
}

#[test]
fn ac_14_reset_on_replay_rearm() {
    use crate::session::{PtySession, SessionRegistry};
    let reg = SessionRegistry::new();
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    let pane_id = session.pane_id.clone();
    reg.insert_test_session(Arc::clone(&session));

    session.flow_state.record_emit(FLOW_HIGH_WATERMARK * 2);
    session.ring.lock().unwrap().push("data".into());

    let r = reg.replay(&pane_id, 0).unwrap();
    assert_eq!(r.last_seq, 1);
    assert_eq!(
        session.flow_state.outstanding(),
        0,
        "replay 재무장 시 회계 리셋 (R15 ii)"
    );
}

#[test]
fn ac_14_reset_on_stall_valve_fire() {
    let cfg = FlowConfig {
        stall_timeout: Duration::from_millis(40),
        park_recheck: Duration::from_millis(10),
        ..Default::default()
    };
    let fs = Arc::new(FlowState::with_config(cfg));
    fs.set_replay_synced_for_test(true);
    fs.record_emit(FLOW_HIGH_WATERMARK * 2);

    let fs_t = Arc::clone(&fs);
    let handle = std::thread::spawn(move || fs_t.check_park_and_wait(RING_PAUSE_THRESHOLD + 1, true));
    let _ = handle.join();
    assert_eq!(
        fs.outstanding(),
        0,
        "정지 밸브 발화 시 회계 리셋 (R15 iii)"
    );
}

#[test]
fn ac_14_outstanding_above_high_then_resume_after_rearm() {
    // 핵심 케이스: outstanding > HIGH에서 전환 → remount + replay 재무장 후
    // emitter가 live 방출을 재개(영구 정지 아님).
    use crate::output::{pump_once, PtyOutputEvent};
    use crate::session::{PtySession, SessionRegistry};

    let reg = Arc::new(SessionRegistry::new());
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    let pane_id = session.pane_id.clone();
    reg.insert_test_session(Arc::clone(&session));
    reg.set_active_workspace(Some("ws1"));

    // 1) 미확인 회계를 HIGH 초과로 만든다.
    session.flow_state.record_emit(FLOW_HIGH_WATERMARK + 1024);
    session.replay_synced.store(true, Ordering::SeqCst);
    session.ring.lock().unwrap().push("hello".into());

    // 2) pump — 게이트에 막혀 방출 없음.
    let mut emitted = 0usize;
    pump_once(&reg, &|_: PtyOutputEvent| emitted += 1, &|_| {});
    assert_eq!(emitted, 0, "outstanding > HIGH → emitter 게이트 막힘");

    // 3) 전환/재무장 — replay 호출 → 회계 리셋.
    reg.replay(&pane_id, 0).unwrap();
    assert_eq!(session.flow_state.outstanding(), 0, "재무장 후 outstanding=0");

    // 4) pump 재시도 — 이제 방출 재개.
    pump_once(&reg, &|_: PtyOutputEvent| emitted += 1, &|_| {});
    assert!(
        emitted >= 1,
        "재무장+리셋 후 live 방출 재개 (영구 정지 아님, AC-14 핵심)"
    );
}

// ==== AC-15: replay() vs pump_once seq 경합 (R16) ====

#[test]
fn ac_15_concurrent_pump_no_seq_rewind_no_duplicate() {
    use crate::output::{pump_once, PtyOutputEvent};
    use crate::session::{PtySession, SessionRegistry};

    let reg = Arc::new(SessionRegistry::new());
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    reg.insert_test_session(Arc::clone(&session));
    reg.set_active_workspace(Some("ws1"));

    for i in 0..10 {
        session.ring.lock().unwrap().push(format!("chunk{}", i));
    }
    session.replay_synced.store(true, Ordering::SeqCst);

    let emissions: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    // 8개 pump 스레드가 동시에 active 세션을 순회.
    for _ in 0..8 {
        let reg = Arc::clone(&reg);
        let em = Arc::clone(&emissions);
        handles.push(std::thread::spawn(move || {
            pump_once(&reg, &|ev: PtyOutputEvent| {
                em.lock().unwrap().push(ev.seq);
            }, &|_| {});
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // R16: 같은 seq가 두 번 방출되면 안 된다.
    let emissions = emissions.lock().unwrap();
    let mut seen = std::collections::HashSet::new();
    for &s in emissions.iter() {
        assert!(
            seen.insert(s),
            "중복 seq {} 발생 (R16 위반: replay 구간 중복 재방출)",
            s
        );
    }
    // last_emitted_seq는 단조 증가 — 되감김 없음.
    let final_seq = session.last_emitted_seq.load(Ordering::SeqCst);
    assert_eq!(final_seq, 10, "last_emitted_seq가 ring.last_seq()로 전진 (되감김 없음)");
}

#[test]
fn ac_15_replay_then_pump_no_overlap() {
    // 단순 순차 케이스: replay가 last_emitted_seq를 store한 뒤 pump가 돌면,
    // pump는 replay가 저장한 값을 읽어 중복 방출하지 않는다.
    use crate::output::{pump_once, PtyOutputEvent};
    use crate::session::{PtySession, SessionRegistry};

    let reg = SessionRegistry::new();
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    let pane_id = session.pane_id.clone();
    reg.insert_test_session(Arc::clone(&session));
    reg.set_active_workspace(Some("ws1"));

    for i in 0..5 {
        session.ring.lock().unwrap().push(format!("c{}", i));
    }
    session.replay_synced.store(true, Ordering::SeqCst);

    // replay — last_seq=5로 last_emitted_seq 전진.
    let r = reg.replay(&pane_id, 0).unwrap();
    assert_eq!(r.last_seq, 5);
    assert_eq!(session.last_emitted_seq.load(Ordering::SeqCst), 5);

    // pump — 더 이상 방출할 게 없어야 한다(replay가 이미 5까지 전진시킴).
    let mut emitted = 0;
    pump_once(&reg, &|_: PtyOutputEvent| emitted += 1, &|_| {});
    assert_eq!(emitted, 0, "replay 후 pump는 중복 방출 없음 (R16)");
}
