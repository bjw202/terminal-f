//! SPEC-PTY-FLOW-001 — PTY 출력 흐름 제어 상태 기계 (M1).
//!
//! 백엔드 세션별 흐름 제어 상태를 소유한다. ack 기반 워터마크(R3) + reader park
//! 게이트(R4) + 정지 안전밸브(R6) + 회계 리셋(R15)을 통합하며, R16 경합 방지를
//! 위해 condvar + Mutex 직렬화 포인트를 제공한다.
//!
//! 본 모듈의 상태는 세션별로 독립이다(R1). condvar용 뮤텍스는 오직 per-session
//! flow 상태만 보호하며, registry 전역 락을 쥔 채 park하지 않는다(spec §C).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

// ==== 명명 상수 (spec §B 시드표) ====

// @MX:ANCHOR: 흐름 제어 워터마크 상수 블록 (SPEC-PTY-FLOW-001 §B).
// @MX:REASON: 16ms 병합 배치 위의 바이트 기반 워터마크. VS Code의 문자 기반
// 100KB/5KB 와는 다른 축 — 본 프로젝트는 16ms 배치 위에서 바이트 단위로 판정한다.
// seed 값 근거는 plan.md §A.1 에 있다.
pub const FLOW_HIGH_WATERMARK: usize = 128 * 1024; // 128 KiB / 131072
pub const FLOW_LOW_WATERMARK: usize = 32 * 1024; // 32 KiB / 32768

// @MX:ANCHOR: reader park 임계값 (SPEC-PTY-FLOW-001 §B R4).
// @MX:REASON: RING_MAX_BYTES(1 MiB)의 75% — UI가 방출을 따라가지 못해 ring이
// 찰 때 reader가 park 하여 ConPTY 파이프를 차오르게 하고 자식 write()를 블로킹.
pub const RING_PAUSE_THRESHOLD: usize = 768 * 1024; // 768 KiB = (RING_MAX_BYTES * 3) / 4

// @MX:ANCHOR: 정지 안전밸브 타임아웃 (R6).
// @MX:REASON: 죽은/웨지된 프론트가 자식 프로세스를 영구 블로킹하지 않도록
// ack 진전이 없으면 폴백(oldest-drop + overflow 배너)로 빠진다.
pub const FLOW_STALL_TIMEOUT: Duration = Duration::from_secs(10);

// @MX:ANCHOR: reader park 재확인 주기 (R4).
// @MX:REASON: condvar signal이 누락되더라지 100ms 내 재평가하여 안전망.
pub const READER_PARK_RECHECK_MS: Duration = Duration::from_millis(100);

/// 흐름 제어 설정. 테스트/벤치 빌드에서 축소 주입(AC-4 타임아웃, B.5 워터마크)을
/// 위해 분리되었으며, 프로덕션 코드는 `FlowConfig::default()` 상수값을 사용한다.
#[derive(Debug, Clone, Copy)]
pub struct FlowConfig {
    pub high_watermark: usize,
    pub low_watermark: usize,
    pub ring_pause_threshold: usize,
    pub stall_timeout: Duration,
    pub park_recheck: Duration,
}

impl Default for FlowConfig {
    fn default() -> Self {
        // SPEC-PTY-FLOW-002 M2 (B10 완화책): TERMF_FLOW_STALL_TIMEOUT_MS 환경변수를
        // 1회 읽어 stall_timeout 을 오버라이드한다. 미설정·파싱 실패 시 기본
        // FLOW_STALL_TIMEOUT(10s) 불변 — 신규 임계값 상수가 아니라 테스트·계측용
        // 오버라이드 창구다(registry 세션이 FlowState::default() 하드코딩이라 config
        // 주입 seam 이 없으므로 env 가 유일한 수단).
        let stall_timeout = std::env::var("TERMF_FLOW_STALL_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(FLOW_STALL_TIMEOUT);
        Self {
            high_watermark: FLOW_HIGH_WATERMARK,
            low_watermark: FLOW_LOW_WATERMARK,
            ring_pause_threshold: RING_PAUSE_THRESHOLD,
            stall_timeout,
            park_recheck: READER_PARK_RECHECK_MS,
        }
    }
}

struct FlowInner {
    /// R8 disarm — teardown 시 설정. 깨어난 reader는 park 조건 재평가 전에 이를 검사.
    disarmed: bool,
    /// R4 reader parked 플래그 — R7 BUSY 판정 및 관측(R1 flow_stats)에 사용.
    reader_parked: bool,
    /// 마지막 ack 진전 시각 — R6 밸브 타이머의 리셋 기준.
    last_ack_at: Instant,
    /// SPEC-PTY-FLOW-002 R7 — emitter 정지 진입 시각 (규칙 0 무장 / 규칙 2 발화 기준).
    paused_since: Option<Instant>,
    /// R7 — 정지 진입/마지막 진전 시점의 acked 스냅샷 (규칙 1 진전 판정 기준).
    paused_at_acked: u64,
}

impl Default for FlowInner {
    fn default() -> Self {
        Self {
            disarmed: false,
            reader_parked: false,
            last_ack_at: Instant::now(),
            paused_since: None,
            paused_at_acked: 0,
        }
    }
}

/// 세션별 흐름 제어 상태 (R1).
///
/// @MX:ANCHOR: outstanding = emitted_bytes.saturating_sub(acked_bytes) 불변식 (R1).
/// @MX:REASON: 미확인 바이트는 항상 위 산식으로만 구한다. saturating_sub 는 R13
/// 방어(acked > emitted 왜곡)로 패닉/래핑 없이 0으로 바닥낸다.
pub struct FlowState {
    /// R1 누적 방출 바이트. u64 포화 없음, 실사용 범위.
    emitted_bytes: AtomicU64,
    /// R1 누적 ack 바이트 (R2 ack_output 으로만 전진).
    acked_bytes: AtomicU64,
    /// R4/R8 condvar용 상태 — 본 뮤텍스는 오직 flow 상태만 감싼다(registry 락과 독립).
    inner: Mutex<FlowInner>,
    condvar: Condvar,
    /// R3 emitter 정지 플래그 (히스테리시스 상태 기억).
    emitter_paused: AtomicBool,
    /// R6 밸브 발화 횟수 (관측/디버그).
    valve_fired: AtomicU64,
    /// SPEC-PTY-FLOW-002 R8 — emitter 정지 밸브 발화 횟수 (R10 관측 노출).
    emitter_valve_fired: AtomicU64,
    /// 설정 — 테스트/벤치에서 축소 주입 가능(spec §C 상수 규율).
    pub config: FlowConfig,
}

impl Default for FlowState {
    fn default() -> Self {
        Self::with_config(FlowConfig::default())
    }
}

impl FlowState {
    pub fn with_config(config: FlowConfig) -> Self {
        Self {
            emitted_bytes: AtomicU64::new(0),
            acked_bytes: AtomicU64::new(0),
            inner: Mutex::new(FlowInner::default()),
            condvar: Condvar::new(),
            emitter_paused: AtomicBool::new(false),
            valve_fired: AtomicU64::new(0),
            emitter_valve_fired: AtomicU64::new(0),
            config,
        }
    }

    // ==== R1 회계 ====

    pub fn emitted(&self) -> u64 {
        self.emitted_bytes.load(Ordering::SeqCst)
    }

    pub fn acked(&self) -> u64 {
        self.acked_bytes.load(Ordering::SeqCst)
    }

    /// @MX:ANCHOR: outstanding = emitted.saturating_sub(acked) 불변식 진입점 (R1).
    pub fn outstanding(&self) -> usize {
        let em = self.emitted();
        let ak = self.acked();
        em.saturating_sub(ak) as usize
    }

    /// R1 emit 시 호출 — 방출한 payload 바이트 수만큼 emitted 를 전진한다.
    /// reader에게 알려 park 조건을 재평가시킨다(ring un-emitted 가 수축).
    pub fn record_emit(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        self.emitted_bytes.fetch_add(bytes as u64, Ordering::SeqCst);
        self.notify_reader();
    }

    /// R2 ack 수신 — acked 에 bytes 를 가산. R6 밸브 타이머를 리셋(진전 신호).
    /// 호출자(commands::ack_output)가 미지 세션을 이미 조용히 무시하므로 본 메서드는
    /// 존재하는 세션에 대해서만 불린다.
    pub fn record_ack(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        self.acked_bytes.fetch_add(bytes, Ordering::SeqCst);
        {
            let mut inner = self.inner.lock().unwrap();
            inner.last_ack_at = Instant::now();
        }
        self.notify_reader();
    }

    fn notify_reader(&self) {
        // lock-then-notify: wait 에 빠진 reader 가 조건을 다시 평가하도록.
        let _guard = self.inner.lock().unwrap();
        self.condvar.notify_all();
    }

    // ==== R3 emitter 게이트 (히스테리시스) ====

    /// 직전 상태(was_paused)와 outstanding 에 따라 방출 여부를 결정한다.
    /// 반환 true → 방출, false → skip. emitter_paused 플래그를 갱신한다.
    ///
    /// @MX:WARN: [AUTO] SPEC-PTY-FLOW-002 R7~R9 — emitter 정지 안전밸브 (규칙 0~3).
    /// @MX:REASON: reader 가 park 하지 않는 사각지대(deficit > LOW 지속 + ring
    /// 미방출 < RING_PAUSE_THRESHOLD)에는 자가 치유 경로가 없어 emitter 정지가
    /// 영구 고착된다. 본 밸브는 stall_timeout 무진전 시 회계를 리셋하는 열화
    /// 경로다 — 1차 해법(단위 통일, M1)을 대체하지 않으며 밸브 발화 관측 시
    /// 누수 경로(B11/B12) 조사가 우선이다(plan §G "밸브를 1차 해법으로 취급 금지").
    /// 임계 구역은 원자 읽기 + 필드 2개 갱신뿐이며 블로킹 대기 없음(B5 락 규율 —
    /// notify_reader 가 이미 같은 뮤텍스를 lock-then-notify 로 사용).
    pub fn emitter_gate_decision(&self, was_paused: bool) -> bool {
        let outstanding = self.outstanding();
        let mut emit = if was_paused {
            // 정지 중 → LOW 이하에서만 재개
            outstanding <= self.config.low_watermark
        } else {
            // 방출 중 → HIGH 초과에서만 정지
            outstanding <= self.config.high_watermark
        };

        // ==== SPEC-PTY-FLOW-002 M2 — emitter 밸브 판정 (plan §A.3 규칙 0~3) ====
        {
            let now = Instant::now();
            let mut inner = self.inner.lock().unwrap();
            if !emit {
                let acked = self.acked();
                match inner.paused_since {
                    None => {
                        // 규칙 0 (무장): 정지 결정 + 미무장 → 무장. 발화 판정은 다음
                        // tick 부터. 전이 직전 was_paused 값과 무관하게 이번 결정이
                        // "정지"이면 무장하므로 (i) 방출→정지 전이와 (ii) 전이 없는
                        // 직접 진입(was_paused=true 최초 호출)을 모두 덮는다.
                        inner.paused_since = Some(now);
                        inner.paused_at_acked = acked;
                    }
                    Some(paused_at) => {
                        if acked > inner.paused_at_acked {
                            // 규칙 1 (진전 리셋, R9): ack 전진 → 재무장, 미발화.
                            inner.paused_since = Some(now);
                            inner.paused_at_acked = acked;
                        } else if now.duration_since(paused_at) >= self.config.stall_timeout {
                            // 규칙 2 (발화, R8): 무진전 + 타임아웃 경과 → 회계 리셋 +
                            // 카운터 + 무장 해제 + 방출 재개. reset_accounting 는 원자
                            // 연산만 사용해 락 중첩이 없다.
                            self.reset_accounting();
                            self.emitter_valve_fired.fetch_add(1, Ordering::SeqCst);
                            inner.paused_since = None;
                            inner.paused_at_acked = 0;
                            emit = true;
                        }
                    }
                }
            } else {
                // 규칙 3 (해제): 방출 결정 → 무장 상태 초기화.
                inner.paused_since = None;
                inner.paused_at_acked = 0;
            }
        }

        self.emitter_paused.store(!emit, Ordering::SeqCst);
        emit
    }

    pub fn is_emitter_paused(&self) -> bool {
        self.emitter_paused.load(Ordering::SeqCst)
    }

    // ==== R4/R5/R8 reader park ====

    /// park 조건 평가 (순수, 블로킹 없음). R5 disarm 이 우선.
    pub fn should_reader_park(&self, un_emitted_bytes: usize, replay_synced: bool) -> bool {
        let inner = self.inner.lock().unwrap();
        if inner.disarmed {
            return false;
        }
        replay_synced && un_emitted_bytes > self.config.ring_pause_threshold
    }

    /// @MX:WARN: reader park 루프 — condvar wait 로 블로킹한다.
    /// @MX:REASON: 자식 프로세스의 write()를 블로킹하여 백엔드 메모리를 보호(ADR-004)
    /// 하면서 활성 팬의 데이터는 유실시키지 않는다. R8 disarm 또는 R6 밸브에서만 해제.
    /// 본 루프는 registry 전역 락이 아닌 flow 전용 뮤텍스만 잡는다(spec §C 락 규율).
    pub fn check_park_and_wait(&self, un_emitted_bytes: usize, replay_synced: bool) -> bool {
        if !self.should_reader_park(un_emitted_bytes, replay_synced) {
            return false;
        }

        let park_recheck = self.config.park_recheck;
        let stall_timeout = self.config.stall_timeout;

        let mut guard = self.inner.lock().unwrap();
        guard.reader_parked = true;
        let mut waited = Duration::ZERO;
        let mut last_progress = guard.last_ack_at;

        loop {
            if guard.disarmed {
                guard.reader_parked = false;
                return true; // R8 disarm 로 해제
            }

            let (new_guard, _wt) = self.condvar.wait_timeout(guard, park_recheck).unwrap();
            guard = new_guard;

            if guard.disarmed {
                guard.reader_parked = false;
                return true;
            }

            // ack 진전이 있으면 밸브 타이머 리셋, 아니면 누적.
            if guard.last_ack_at > last_progress {
                last_progress = guard.last_ack_at;
                waited = Duration::ZERO;
            } else {
                waited += park_recheck;
            }

            if waited >= stall_timeout {
                // R6 정지 밸브 발화 — R15 회계 리셋 동반.
                self.fire_valve_internal(&mut guard);
                return true;
            }
        }
    }

    pub fn is_reader_parked(&self) -> bool {
        self.inner.lock().unwrap().reader_parked
    }

    // ==== R15 회계 리셋 ====

    /// 회계 리셋 — `acked := emitted` (outstanding = 0).
    /// 3지점에서 호출: (i) replay_synced=false 전이, (ii) replay 재무장, (iii) 밸브 발화.
    /// 좌초 outstanding 이 emitter 게이트를 영구 잠그는 것을 막는다(AC-14 핵심).
    pub fn reset_accounting(&self) {
        let em = self.emitted_bytes.load(Ordering::SeqCst);
        self.acked_bytes.store(em, Ordering::SeqCst);
    }

    // ==== R8 teardown disarm ====

    /// teardown 시 호출 — condvar signal **이전에** disarm 플래그를 설정한다.
    /// park 중이던 reader는 깨어난 뒤 park 조건 재평가 전에 disarm 를 먼저 검사하여
    /// 재park 없이 반환한다(AC-5). teardown 은 replay_synced 를 지우지 않으므로
    /// disarm 선행이 필수다(spec R8).
    pub fn disarm_for_teardown(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.disarmed = true;
        guard.reader_parked = false;
        self.condvar.notify_all();
    }

    // ==== R6 정지 밸브 ====

    fn fire_valve_internal(&self, guard: &mut std::sync::MutexGuard<'_, FlowInner>) {
        guard.reader_parked = false;
        // R15 회계 리셋 — 밸브 발화 시 동반.
        let em = self.emitted_bytes.load(Ordering::SeqCst);
        self.acked_bytes.store(em, Ordering::SeqCst);
        guard.last_ack_at = Instant::now();
        self.valve_fired.fetch_add(1, Ordering::SeqCst);
    }

    pub fn valve_fired_count(&self) -> u64 {
        self.valve_fired.load(Ordering::SeqCst)
    }

    /// SPEC-PTY-FLOW-002 R8 — emitter 정지 밸브 발화 횟수 (테스트/관측 창구).
    pub fn emitter_valve_fired_count(&self) -> u64 {
        self.emitter_valve_fired.load(Ordering::SeqCst)
    }

    // ==== R1 관측 (flow_stats 커맨드 원천) ====

    pub fn flow_stats(&self) -> FlowStats {
        let inner = self.inner.lock().unwrap();
        FlowStats {
            emitted: self.emitted(),
            acked: self.acked(),
            outstanding: self.outstanding(),
            emitter_paused: self.emitter_paused.load(Ordering::SeqCst),
            reader_parked: inner.reader_parked,
            // SPEC-PTY-FLOW-002 R10 — 두 밸브 카운터 신규 노출(append-only).
            // valve_fired 는 기존 내부 카운터의 값 로직 불변 노출.
            valve_fired: self.valve_fired.load(Ordering::SeqCst),
            emitter_valve_fired: self.emitter_valve_fired.load(Ordering::SeqCst),
        }
    }

    // ==== 테스트 전용 헬퍼 ====

    #[cfg(test)]
    pub fn set_replay_synced_for_test(&self, _v: bool) {
        // no-op: replay_synced 는 PtySession 필드이며 check_park_and_wait 에게
        // 파라미터로 전달된다. 본 메서드는 테스트 API 대칭을 위해 존재한다.
    }

    #[cfg(test)]
    pub fn set_parked_for_test(&self, parked: bool) {
        self.inner.lock().unwrap().reader_parked = parked;
    }
}

/// 흐름 제어 관측 정보(R1 — 프론트/autotest/bench 관측 창구).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStats {
    pub emitted: u64,
    pub acked: u64,
    pub outstanding: usize,
    pub emitter_paused: bool,
    pub reader_parked: bool,
    /// SPEC-PTY-FLOW-002 R10 — reader-park 밸브(R6) 발화 횟수 (신규 노출).
    pub valve_fired: u64,
    /// SPEC-PTY-FLOW-002 R10 — emitter 정지 밸브(R8) 발화 횟수 (신규).
    pub emitter_valve_fired: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec_constants() {
        let cfg = FlowConfig::default();
        assert_eq!(cfg.high_watermark, 128 * 1024);
        assert_eq!(cfg.low_watermark, 32 * 1024);
        assert_eq!(cfg.ring_pause_threshold, 768 * 1024);
        assert_eq!(cfg.stall_timeout, Duration::from_secs(10));
        assert_eq!(cfg.park_recheck, Duration::from_millis(100));
    }
}
