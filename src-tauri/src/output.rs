//! Batched output emission (ADR-004 backpressure design).
//!
//! Instead of emitting one Tauri event per PTY chunk (which floods the IPC
//! bridge under heavy output), a single emitter thread wakes every
//! `EMIT_INTERVAL_MS`, scans the sessions of the *active* workspace, and
//! emits at most one coalesced event per pane per tick. Inactive workspaces
//! never emit; their output stays in the per-session ring buffer and is
//! replayed on workspace switch.
//!
//! SPEC-PTY-FLOW-001 M1 — emitter 워터마크 게이트(R3)가 추가되었다. 미확인 바이트
//! (outstanding = emitted - acked)가 FLOW_HIGH_WATERMARK 를 넘으면 방출을 건너뛰고,
//! FLOW_LOW_WATERMARK 이하로 내려오면 재개한다(히스테리시스). R16 경합 방지를 위해
//! `collect_since` + `last_emitted_seq.store` 가 동일한 ring 락 범위에서 수행된다.

use crate::model::{PaneId, SessionId, WorkspaceId};
use crate::session::{Lifecycle, SessionRegistry};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub const EMIT_INTERVAL_MS: u64 = 16;

// SPEC-PTY-FLOW-001 §B — 워터마크 상수는 flow_state 모듈에 정의되고 본 모듈이 사용한다.
// (spec 의 "위치: output.rs" 표기는 물리적 파일이 아닌 'emitter가 사용한다'는 의미.)
pub use crate::flow_state::{FLOW_HIGH_WATERMARK, FLOW_LOW_WATERMARK};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOutputEvent {
    pub workspace_id: WorkspaceId,
    pub pane_id: PaneId,
    pub session_id: SessionId,
    /// Sequence number of the last chunk included in `data`.
    pub seq: u64,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyExitEvent {
    pub workspace_id: WorkspaceId,
    pub pane_id: PaneId,
    pub session_id: SessionId,
    pub exit_code: Option<u32>,
}

/// One emitter pass. Extracted from the thread loop so tests and the
/// benchmark can drive it deterministically. Returns emitted event count.
///
/// SPEC-PTY-FLOW-001:
/// - R3: 세션별 워터마크 게이트(히스테리시스)로 방출 여부를 결정한다.
/// - R16: `collect_since` + `last_emitted_seq.store` 가 동일 ring 락 안에서 수행되어
///   `replay()` 의 collect+store 와 직렬화된다(seq 되감김·중복 재방출 방지, AC-15).
/// - R1: 방출한 payload 바이트 수만큼 `flow_state.record_emit` 으로 emitted 를 전진.
pub fn pump_once<FO, FE>(registry: &SessionRegistry, emit_output: &FO, emit_exit: &FE) -> usize
where
    FO: Fn(PtyOutputEvent),
    FE: Fn(PtyExitEvent),
{
    let mut emitted = 0usize;
    for session in registry.active_sessions_snapshot() {
        if session.replay_synced.load(Ordering::SeqCst) {
            // R3 워터마크 게이트 — 히스테리시스 상태는 flow_state 에 보존된다.
            let was_paused = session.flow_state.is_emitter_paused();
            let should_emit = session.flow_state.emitter_gate_decision(was_paused);

            if should_emit {
                // R16: collect + store 를 동일 ring 락 안에서 수행.
                let batch_opt = {
                    let ring = session.ring.lock().unwrap();
                    let last_emitted = session.last_emitted_seq.load(Ordering::SeqCst);
                    if ring.last_seq() > last_emitted {
                        let (data, last_seq, dropped) = ring.collect_since(last_emitted);
                        session.last_emitted_seq.store(last_seq, Ordering::SeqCst);
                        Some((data, last_seq, dropped))
                    } else {
                        None
                    }
                };
                if let Some((data, last_seq, dropped)) = batch_opt {
                    let data = if dropped {
                        format!(
                            "\r\n\x1b[33m[terminal-f: output overflow, oldest chunks dropped]\x1b[0m\r\n{data}"
                        )
                    } else {
                        data
                    };
                    // R1: 방출한 바이트만큼 emitted 전진. reader 에게 park 재평가 통지.
                    // 배너 접두어 길이는 흐름 회계에서 제외(백엔드 발생분이 아니라 UI 힌트).
                    session.flow_state.record_emit(data.len());
                    emit_output(PtyOutputEvent {
                        workspace_id: session.workspace_id.clone(),
                        pane_id: session.pane_id.clone(),
                        session_id: session.session_id.clone(),
                        seq: last_seq,
                        data,
                    });
                    emitted += 1;
                }
            }
        }
        let state = *session.state.lock().unwrap();
        if state == Lifecycle::Exited && !session.exit_notified.swap(true, Ordering::SeqCst) {
            emit_exit(PtyExitEvent {
                workspace_id: session.workspace_id.clone(),
                pane_id: session.pane_id.clone(),
                session_id: session.session_id.clone(),
                exit_code: *session.exit_code.lock().unwrap(),
            });
            emitted += 1;
        }
    }
    emitted
}

pub fn start_emitter<FO, FE>(
    registry: Arc<SessionRegistry>,
    emit_output: FO,
    emit_exit: FE,
) -> JoinHandle<()>
where
    FO: Fn(PtyOutputEvent) + Send + 'static,
    FE: Fn(PtyExitEvent) + Send + 'static,
{
    std::thread::Builder::new()
        .name("pty-output-emitter".into())
        .spawn(move || loop {
            pump_once(&registry, &emit_output, &emit_exit);
            std::thread::sleep(Duration::from_millis(EMIT_INTERVAL_MS));
        })
        .expect("failed to start emitter thread")
}
