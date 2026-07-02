//! Batched output emission (ADR-004 backpressure design).
//!
//! Instead of emitting one Tauri event per PTY chunk (which floods the IPC
//! bridge under heavy output), a single emitter thread wakes every
//! `EMIT_INTERVAL_MS`, scans the sessions of the *active* workspace, and
//! emits at most one coalesced event per pane per tick. Inactive workspaces
//! never emit; their output stays in the per-session ring buffer and is
//! replayed on workspace switch.

use crate::model::{PaneId, SessionId, WorkspaceId};
use crate::session::{Lifecycle, SessionRegistry};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub const EMIT_INTERVAL_MS: u64 = 16;

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
pub fn pump_once<FO, FE>(registry: &SessionRegistry, emit_output: &FO, emit_exit: &FE) -> usize
where
    FO: Fn(PtyOutputEvent),
    FE: Fn(PtyExitEvent),
{
    let mut emitted = 0usize;
    for session in registry.active_sessions_snapshot() {
        if session.replay_synced.load(Ordering::SeqCst) {
            let last_emitted = session.last_emitted_seq.load(Ordering::SeqCst);
            let batch = {
                let ring = session.ring.lock().unwrap();
                if ring.last_seq() > last_emitted {
                    Some(ring.collect_since(last_emitted))
                } else {
                    None
                }
            };
            if let Some((data, last_seq, dropped)) = batch {
                session.last_emitted_seq.store(last_seq, Ordering::SeqCst);
                let data = if dropped {
                    format!("\r\n\x1b[33m[terminal-f: output overflow, oldest chunks dropped]\x1b[0m\r\n{data}")
                } else {
                    data
                };
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
