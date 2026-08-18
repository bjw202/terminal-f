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
    // emitter_gate_decision 은 emit 여부(true=방출)를 반환하므로,
    // "현재 정지 상태인가"는 !decision 이다.
    let mut paused = !fs.emitter_gate_decision(false);
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
    fs.record_emit(FLOW_LOW_WATERMARK + (FLOW_HIGH_WATERMARK - FLOW_LOW_WATERMARK) / 2);
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

#[test]
fn ac_3_reader_park_gate_uses_bytes_not_chunks() {
    // 회귀 재현(R4 단위 불일치 버그): 청크 1개지만 RING_PAUSE_THRESHOLD 초과 **바이트** →
    // 게이트 조건이 true 여야 한다. 과거 버그: check_reader_park_gate 가 청크 수(seq 차이,
    // 최대 ~1024)를 써서 1 < 786432 → 항상 false — reader park 가 실제로 발동하지 않았음.
    use crate::session::PtySession;
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    session.replay_synced.store(true, Ordering::SeqCst);
    let big = "x".repeat(RING_PAUSE_THRESHOLD + 1024);
    session.ring.lock().unwrap().push(big); // 청크 1개, 바이트는 임계 초과
    assert!(
        session.reader_should_park_now(),
        "R4 reader park 게이트는 미방출 **바이트** 기준이어야 한다 (청크 수 아님)"
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
        stall_timeout: Duration::from_millis(400),
        park_recheck: Duration::from_millis(50),
        ..Default::default()
    };
    let fs = Arc::new(FlowState::with_config(cfg));
    fs.set_replay_synced_for_test(true);
    fs.record_emit(RING_PAUSE_THRESHOLD + 1024);

    // worker는 park loop에 진입 — ack 진전이 stall 타이머를 리셋하는지 확인.
    // 매 50ms마다 wait_timeout → 400ms 누적 시 valve 발화. 그 전에 ack를 주면
    // last_ack_at이 갱신되어 타이머가 리셋 → valve가 바로 발화하지 않음.
    let fs_worker = Arc::clone(&fs);
    let handle = std::thread::spawn(move || {
        fs_worker.check_park_and_wait(RING_PAUSE_THRESHOLD + 1024, true)
    });
    // worker가 park에 들어간 뒤 ack 주입.
    std::thread::sleep(Duration::from_millis(80));
    let t0 = Instant::now();
    fs.record_ack(1024);
    // ack 후 단기간(150ms)에는 valve가 발화하지 않아야 한다(stall_timeout=400ms 대비).
    std::thread::sleep(Duration::from_millis(150));
    let early = fs.valve_fired_count();
    let early_elapsed = t0.elapsed();
    let _ = handle.join();
    assert_eq!(
        early, 0,
        "ack 진전 후 단기간(150ms < stall 400ms)에는 valve 발화하지 않음"
    );
    assert!(
        early_elapsed < Duration::from_millis(300),
        "단기간 확인 구간 초과하지 않음 (경과: {:?})",
        early_elapsed
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
    let emitted: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    pump_once(
        &reg,
        &|_: PtyOutputEvent| *emitted.lock().unwrap() += 1,
        &|_| {},
    );
    assert_eq!(*emitted.lock().unwrap(), 0, "outstanding > HIGH → emitter 게이트 막힘");

    // 3) 전환/재무장 — replay 호출 → 회계 리셋.
    reg.replay(&pane_id, 0).unwrap();
    assert_eq!(session.flow_state.outstanding(), 0, "재무장 후 outstanding=0");

    // 3.5) reset 후 새 live 데이터 도착 — replay가 last_emitted_seq를 끝까지
    // 전진시켰으므로, "재개"를 보이려면 새 청크가 필요하다 (AC-14 핵심: 영구 정지 아님).
    session.ring.lock().unwrap().push("world".into());

    // 4) pump 재시도 — 이제 방출 재개.
    pump_once(
        &reg,
        &|_: PtyOutputEvent| *emitted.lock().unwrap() += 1,
        &|_| {},
    );
    assert!(
        *emitted.lock().unwrap() >= 1,
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
    let emitted: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    pump_once(
        &reg,
        &|_: PtyOutputEvent| *emitted.lock().unwrap() += 1,
        &|_| {},
    );
    assert_eq!(*emitted.lock().unwrap(), 0, "replay 후 pump은 중복 방출 없음 (R16)");
}

// ==== SPEC-PTY-FLOW-002 M1 — ack 단위 불일치 재현 + 단위 통일 검증 (R1~R3, R12) ====

/// 배선 수준 재현(AC-2): 비ASCII 페이로드를 `pump_once` 경로로 방출하고 이벤트
/// `data` 의 UTF-16 코드 유닛 수로 ack(수정 이전 프론트엔드 동작 모사)하면
/// outstanding 바닥값이 누적 결손만큼 남아 게이트가 정지 상태로 고착된다.
/// ack 는 `record_ack` 직접 호출이 아니라 `reg.ack_output`(IPC 명령 표면)로
/// 전달한다(R12 — 배선 우회 금지, 선행 R4 교훈).
#[test]
fn flow002_ac2_utf16_unit_repro_permanent_pause() {
    use crate::output::{pump_once, PtyOutputEvent};
    use crate::session::{PtySession, SessionRegistry};

    let reg = SessionRegistry::new();
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    let pane_id = session.pane_id.clone();
    reg.insert_test_session(Arc::clone(&session));
    reg.set_active_workspace(Some("ws1"));
    session.replay_synced.store(true, Ordering::SeqCst);

    // '가' 는 UTF-8 3바이트 / UTF-16 1유닛 → 문자당 2바이트 결손.
    // 70000자 = 210000바이트 방출, UTF-16 ack 70000유닛 → 결손 140000바이트가
    // HIGH(128KiB)·LOW(32KiB) 모두 상회하는 영구 잔존 바닥값이 된다.
    let chunk = "가".repeat(70_000);
    session.ring.lock().unwrap().push(chunk);

    let events: Arc<Mutex<Vec<PtyOutputEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    pump_once(
        &reg,
        &|ev: PtyOutputEvent| sink.lock().unwrap().push(ev),
        &|_| {},
    );
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1, "단일 청크 → 단일 병합 이벤트");
    let ev = &events[0];
    let emitted_total = session.flow_state.emitted();
    assert_eq!(emitted_total, 210_000, "record_emit 은 UTF-8 바이트로 가산");

    // 구 프론트엔드 동작 모사: data.length(UTF-16 코드 유닛 수)로 ack.
    let utf16_units = ev.data.encode_utf16().count() as u64;
    assert_eq!(utf16_units, 70_000);
    reg.ack_output(&pane_id, utf16_units).unwrap();

    let outstanding = session.flow_state.outstanding();
    assert_eq!(
        outstanding, 140_000,
        "UTF-16 단위 ack → 누적 결손이 outstanding 바닥값으로 잔존"
    );

    // 다음 tick 게이트 결정 — 결손이 HIGH 를 넘어 정지로 전이.
    let was_paused = session.flow_state.is_emitter_paused();
    let decision = session.flow_state.emitter_gate_decision(was_paused);
    assert!(!decision, "outstanding(결손) > HIGH → 정지 전이");
    assert!(session.flow_state.is_emitter_paused(), "정지 플래그 설정");

    // 영구 정지 시연: 신규 데이터가 와도 정지 상태에서는 방출이 전혀 일어나지 않는다.
    session.ring.lock().unwrap().push("가".repeat(1024));
    pump_once(&reg, &|_ev: PtyOutputEvent| {}, &|_| {});
    assert_eq!(
        session.flow_state.emitted(),
        emitted_total,
        "정지 고착 — 이후 pump 에서 방출 없음 (영구 정지)"
    );
    assert!(
        session.flow_state.outstanding() > FLOW_LOW_WATERMARK,
        "결손 바닥값이 LOW 이하로 내려갈 수 없음 → 재개 불가"
    );
}

/// 짝 테스트(AC-3, GREEN): 동일 배선 경로를 이벤트 `byteLen` 으로 ack하면
/// outstanding 이 FLOW_LOW_WATERMARK 이하(여기선 정확히 0)에 도달하고 게이트가
/// 방출 재개를 반환한다. 이모지(UTF-8 4바이트/UTF-16 2유닛) 포함 페이로드로
/// 서로게이트 페어에서도 반사 ack 가 정확히 균형을 이루는지 함께 검증(§C 엣지).
/// 수정 이전에는 `byte_len` 필드 부재로 컴파일 실패 — RED 의 첫 형태(AC-2).
#[test]
fn flow002_ac3_bytelen_ack_drains_outstanding_and_resumes() {
    use crate::output::{pump_once, PtyOutputEvent};
    use crate::session::{PtySession, SessionRegistry};

    let reg = SessionRegistry::new();
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    let pane_id = session.pane_id.clone();
    reg.insert_test_session(Arc::clone(&session));
    reg.set_active_workspace(Some("ws1"));
    session.replay_synced.store(true, Ordering::SeqCst);

    // '가'(3B/1u) + '─'(3B/1u) + '😀'(4B/2u) = 단위당 10바이트/4유닛.
    // 20000반복 = 200000바이트 > HIGH(128KiB) → 1회 방출 후 정지 조건 성립.
    let chunk = "가─😀".repeat(20_000);
    session.ring.lock().unwrap().push(chunk);

    let events: Arc<Mutex<Vec<PtyOutputEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    pump_once(
        &reg,
        &|ev: PtyOutputEvent| sink.lock().unwrap().push(ev),
        &|_| {},
    );
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let ev = &events[0];

    // R1/R2: 이벤트 byteLen == 최종 data 의 UTF-8 바이트 길이 == emit 회계 가산값.
    assert_eq!(ev.byte_len, ev.data.len());
    assert_eq!(ev.data.len(), 200_000);
    assert_eq!(session.flow_state.emitted(), 200_000);

    // 게이트 정지 전이(outstanding > HIGH).
    assert!(
        !session.flow_state.emitter_gate_decision(false),
        "HIGH 초과 → 정지 전이"
    );
    assert!(session.flow_state.is_emitter_paused());

    // 수정된 프론트엔드 동작: 이벤트 byteLen 으로 ack.
    reg.ack_output(&pane_id, ev.byte_len as u64).unwrap();

    // 서로게이트 페어 포함 페이로드에서 반사 ack 후 outstanding = 0 (§C 엣지).
    assert_eq!(session.flow_state.outstanding(), 0);
    assert!(
        session.flow_state.emitter_gate_decision(true),
        "outstanding 0 <= LOW → 방출 재개"
    );
    assert!(
        !session.flow_state.is_emitter_paused(),
        "재개 후 정지 플래그 해제"
    );
}

/// AC-1(배너 케이스): 오버플로 배너가 붙은 이벤트에서도 record_emit 가산값과
/// 이벤트 byteLen 이 동일한 최종 문자열(배너 포함)에서 산출된다.
#[test]
fn flow002_ac1_banner_included_bytelen_same_source_as_emit() {
    use crate::output::{pump_once, PtyOutputEvent};
    use crate::session::{PtySession, SessionRegistry};

    let reg = SessionRegistry::new();
    let session = Arc::new(PtySession::new_test("ws1", "pane1"));
    reg.insert_test_session(Arc::clone(&session));
    reg.set_active_workspace(Some("ws1"));
    session.replay_synced.store(true, Ordering::SeqCst);

    // 1.2MiB 푸시 → RING_MAX_BYTES(1MiB) 초과로 가장 오래된 청크 퇴거 →
    // collect_since 가 dropped=true 를 반환하고 배너가 접두된다.
    for _ in 0..3 {
        session.ring.lock().unwrap().push("a".repeat(400_000));
    }
    let emitted_before = session.flow_state.emitted();

    let events: Arc<Mutex<Vec<PtyOutputEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    pump_once(
        &reg,
        &|ev: PtyOutputEvent| sink.lock().unwrap().push(ev),
        &|_| {},
    );
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let ev = &events[0];

    assert!(
        ev.data
            .contains("[terminal-f: output overflow, oldest chunks dropped]"),
        "오버플로 배너 포함"
    );
    // 배너 포함 최종 문자열 기준 — 배너 없는 데이터(800000바이트)보다 길다.
    assert!(ev.data.len() > 800_000, "배너 바이트가 data 에 포함됨");
    // 동일 원천(R2): 이벤트 byteLen == 최종 data 바이트 길이 == emit 회계 가산값.
    assert_eq!(ev.byte_len, ev.data.len());
    assert_eq!(
        session.flow_state.emitted() - emitted_before,
        ev.byte_len as u64,
        "record_emit 가산값 == 이벤트 byteLen (배너 포함)"
    );
}

// ==== SPEC-PTY-FLOW-002 M2 — emitter 정지 안전밸브 (R7~R10, AC-6/7/8) ====

/// 밸브용 축소 FlowConfig (B.5 주입 패턴 계승): 작은 워터마크 + 짧은 stall_timeout.
/// 실시간 10초 대기 금지(§G) — 60ms 로 결정론적 검증.
fn flow002_valve_config() -> FlowConfig {
    FlowConfig {
        high_watermark: 8 * 1024,
        low_watermark: 2 * 1024,
        stall_timeout: Duration::from_millis(60),
        ..Default::default()
    }
}

/// AC-6 (경로 i — 방출→정지 전이): emitter 정지 + ack 무진전 상태로
/// `stall_timeout` 경과 후 밸브가 발화하여 회계가 리셋(outstanding=0)되고
/// 게이트가 방출 재개를 반환한다. 정지 진입 tick(무장 tick, 규칙 0)에는
/// 발화하지 않는다.
#[test]
fn flow002_ac6_emitter_valve_fires_after_stall_no_ack_progress() {
    let fs = FlowState::with_config(flow002_valve_config());
    fs.record_emit(16 * 1024); // outstanding(16KiB) > HIGH(8KiB)

    // 방출 → 정지 전이 tick: 이 tick 이 무장 tick 이며 발화하지 않는다(규칙 0).
    let decision_arm = fs.emitter_gate_decision(false);
    assert!(!decision_arm, "outstanding > HIGH → 정지 전이");
    assert_eq!(
        fs.emitter_valve_fired_count(),
        0,
        "무장 tick 에는 발화하지 않는다 (규칙 0)"
    );

    // outstanding 은 그대로(ack 없음) → 정지 유지. stall_timeout(60ms) 경과 대기.
    std::thread::sleep(Duration::from_millis(90));

    // 무진전 + 타임아웃 경과 → 밸브 발화: 회계 리셋 + 방출 재개(규칙 2).
    let decision_fire = fs.emitter_gate_decision(true);
    assert!(
        decision_fire,
        "밸브 발화 → 회계 리셋으로 outstanding=0 → 방출 재개"
    );
    assert_eq!(fs.outstanding(), 0, "밸브 발화 → R15 회계 리셋");
    assert_eq!(fs.emitter_valve_fired_count(), 1, "발화 카운터 증가");
    assert!(!fs.is_emitter_paused(), "발화 후 정지 플래그 해제");
}

/// AC-6 (경로 ii — 전이 없이 정지 상태로 직접 진입): 기존 테스트가
/// `emitter_gate_decision` 을 상태 전이 없이 직접 호출하던 패턴
/// (flow_tests.rs:22-45)을 반영. was_paused=true 로 직접 진입한 첫 tick 도
/// 규칙 0 무장으로 처리되어 발화하지 않으며, 이후 stall_timeout 경과 후 발화한다.
#[test]
fn flow002_ac6_emitter_valve_direct_paused_entry_path() {
    let fs = FlowState::with_config(flow002_valve_config());
    fs.record_emit(16 * 1024);

    // 전이 없이 정지 상태로 직접 진입 — paused_since 가 None 인 채 정지 관측.
    let decision_arm = fs.emitter_gate_decision(true);
    assert!(!decision_arm, "outstanding(16KiB) > LOW(2KiB) → 정지 유지");

    std::thread::sleep(Duration::from_millis(90));

    let decision_fire = fs.emitter_gate_decision(true);
    assert!(decision_fire, "직접 진입 경로에서도 무장 후 stall 경과 → 발화");
    assert_eq!(fs.outstanding(), 0, "발화 → 회계 리셋");
}

/// AC-7 (부정 테스트, R9): emitter 정지 중 ack 이 (느리게라도) 계속 전진하면
/// 매 tick 재무장되어 stall_timeout 을 훨씬 넘긴 시간이 흘러도 밸브이 발화하지
/// 않는다. 느린 팬의 미확인 구간이 회계상 삭제되지 않음을 보장(B3).
#[test]
fn flow002_ac7_emitter_valve_no_fire_while_ack_progressing() {
    let fs = FlowState::with_config(flow002_valve_config());
    fs.record_emit(16 * 1024);
    assert!(!fs.emitter_gate_decision(false), "정지 전이 + 무장");

    // 매 반복: stall_timeout(60ms)보다 긴 70ms 대기 후 ack 진전 → 게이트 호출.
    // 진전이 있으므로 매번 타이머가 리셋(규칙 1)되어 발화하지 않는다.
    let mut acked_total = 0u64;
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(70));
        fs.record_ack(1024);
        acked_total += 1024;
        let decision = fs.emitter_gate_decision(true);
        assert!(!decision, "outstanding 여전히 LOW 초과 → 정지 유지");
    }
    // 총 560ms 경과 — stall_timeout(60ms)의 9배. 진전 중 발화 없음.
    let outstanding = fs.outstanding() as u64;
    assert_eq!(
        outstanding,
        16 * 1024 - acked_total,
        "미확인 구간이 회계상 삭제되지 않음 (밸브 미발화)"
    );
    assert_eq!(fs.emitter_valve_fired_count(), 0, "진전 중 밸브 미발화 (R9)");
}

/// AC-8 (R10): `flow_stats` 응답에 `valveFired`(reader-park 밸브, 기존 카운터
/// 노출)와 `emitterValveFired`(emitter 밸브, 신규 카운터) 두 필드가 신규 노출되고
/// emitter 밸브 발화가 그 값으로 관측된다. 기존 5필드는 이름·타입·의미 불변(R11).
#[test]
fn flow002_ac8_flow_stats_exposes_valve_counters() {
    let fs = FlowState::with_config(flow002_valve_config());

    // 발화 전: 두 카운터 모두 0 노출.
    let before = fs.flow_stats();
    assert_eq!(before.emitter_valve_fired, 0);
    assert_eq!(before.valve_fired, 0);

    fs.record_emit(16 * 1024);
    assert!(!fs.emitter_gate_decision(false), "정지 전이 + 무장");
    std::thread::sleep(Duration::from_millis(90));
    assert!(fs.emitter_gate_decision(true), "밸브 발화");

    let after = fs.flow_stats();
    assert_eq!(after.emitter_valve_fired, 1, "발화가 flow_stats 로 관측");
    assert_eq!(after.valve_fired, 0, "reader-park 밸브는 미발화 — 두 밸브 구분");
    // 기존 5필드 불변 (R11 append-only) — 값은 회계 리셋 후 상태를 반영.
    assert_eq!(after.emitted, 16 * 1024);
    assert_eq!(after.acked, 16 * 1024);
    assert_eq!(after.outstanding, 0);
    assert!(!after.emitter_paused);
    assert!(!after.reader_parked);

    // 직렬화 키 camelCase 검증 — TS FlowStats 인터페이스와의 계약.
    let json = serde_json::to_string(&after).expect("FlowStats 직렬화");
    assert!(json.contains("\"valveFired\":0"), "serde camelCase 키: valveFired");
    assert!(
        json.contains("\"emitterValveFired\":1"),
        "serde camelCase 키: emitterValveFired"
    );
}
