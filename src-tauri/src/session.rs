//! Backend-owned PTY session registry (ADR-001).
//!
//! The backend is the single source of truth for PTY session ownership.
//! Sessions survive workspace switches (keep-alive); they are terminated only
//! when their pane is closed, their workspace is deleted, or the app exits.
//!
//! Output flow: a dedicated reader thread per session reads PTY output in
//! ~8KB chunks, repairs UTF-8 chunk boundaries, and appends to a bounded
//! ring buffer (ADR-004). The ring buffer never blocks the reader: when full
//! it drops the oldest chunks and counts them. Event emission to the frontend
//! is decoupled (see `output.rs`) so a slow UI can never grow backend memory.

use crate::model::{new_id, PaneId, SessionId, WorkspaceId};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Target PTY read chunk size (ADR-004 suggested default ~8KB).
pub const READ_CHUNK_SIZE: usize = 8192;
/// Per-pane ring buffer budget: 1 MiB or 1024 chunks, whichever hits first.
pub const RING_MAX_BYTES: usize = 1024 * 1024;
pub const RING_MAX_CHUNKS: usize = 1024;
/// Live PTY soft cap (ADR-005): spawns beyond this are refused with an error.
pub const LIVE_PTY_SOFT_CAP: usize = 32;

pub const SHELL_CANDIDATES: [&str; 3] = ["pwsh", "powershell", "cmd"];

/// Injection idle gate default: the target pane must have produced no output
/// for this long before an injection is allowed (roadmap §2.3 #2).
pub const INJECT_IDLE_MS: u64 = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    Starting,
    Running,
    Exited,
    Closing,
}

/// Bounded ring buffer of output chunks with monotonic sequence numbers.
/// This is the authoritative raw-output store for replay. It is bounded:
/// overflow drops the oldest chunks (counted, surfaced to the UI as a
/// "dropped" flag on replay). Full raw preservation is NOT guaranteed —
/// a file spool would be needed for that (out of M0 scope, see ADR-004).
pub struct RingBuffer {
    entries: VecDeque<(u64, String)>,
    bytes: usize,
    max_bytes: usize,
    max_chunks: usize,
    next_seq: u64,
    pub dropped_chunks: u64,
    pub dropped_bytes: u64,
    /// Cumulative bytes ever pushed (for throughput measurement).
    pub total_bytes: u64,
}

impl RingBuffer {
    pub fn new(max_bytes: usize, max_chunks: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            max_bytes,
            max_chunks,
            next_seq: 1,
            dropped_chunks: 0,
            dropped_bytes: 0,
            total_bytes: 0,
        }
    }

    pub fn push(&mut self, data: String) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.bytes += data.len();
        self.total_bytes += data.len() as u64;
        self.entries.push_back((seq, data));
        while self.bytes > self.max_bytes || self.entries.len() > self.max_chunks {
            if let Some((_, dropped)) = self.entries.pop_front() {
                self.bytes -= dropped.len();
                self.dropped_bytes += dropped.len() as u64;
                self.dropped_chunks += 1;
            } else {
                break;
            }
        }
        seq
    }

    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// Concatenate all chunks with seq > from_seq.
    /// Returns (data, last_seq, dropped) where dropped=true means chunks in
    /// the requested range were evicted from the ring before being read.
    pub fn collect_since(&self, from_seq: u64) -> (String, u64, bool) {
        let mut out = String::new();
        for (seq, data) in &self.entries {
            if *seq > from_seq {
                out.push_str(data);
            }
        }
        let dropped = match self.entries.front() {
            Some((first_seq, _)) => from_seq + 1 < *first_seq,
            None => from_seq < self.last_seq(),
        };
        (out, self.last_seq(), dropped)
    }
}

pub struct PtySession {
    pub session_id: SessionId,
    pub workspace_id: WorkspaceId,
    pub pane_id: PaneId,
    pub command: String,
    pub cwd: String,
    pub state: Mutex<Lifecycle>,
    pub exit_code: Mutex<Option<u32>>,
    pub ring: Mutex<RingBuffer>,
    pub writer: Mutex<Option<Box<dyn Write + Send>>>,
    pub master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    pub child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
    pub reader_join: Mutex<Option<JoinHandle<()>>>,
    /// Last seq delivered to the frontend via events. The emitter only runs
    /// once `replay_synced` is set by an explicit replay, which establishes
    /// ordering across the snapshot/replay/live-event boundary.
    pub last_emitted_seq: AtomicU64,
    pub replay_synced: AtomicBool,
    pub exit_notified: AtomicBool,
    /// Instant of the most recent PTY output chunk (idle-gate input).
    pub last_output_at: Mutex<std::time::Instant>,
    /// Whether the app in this PTY has enabled bracketed paste (DECSET 2004),
    /// tracked from the output stream like a real terminal does. Injected
    /// prompts are wrapped in ESC[200~ / ESC[201~ only when enabled.
    pub bracketed_paste: AtomicBool,
    /// Control-API output observation (M2.2): when set, the reader also
    /// appends decoded output to `spool`.
    pub observe: AtomicBool,
    pub spool: Mutex<Option<crate::spool::SpoolWriter>>,
    /// Precomputed spool file path for this session.
    pub spool_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub pane_id: PaneId,
    pub session_id: SessionId,
    pub workspace_id: WorkspaceId,
    pub state: Lifecycle,
    pub exit_code: Option<u32>,
    /// Program spawned in this session (for pane header titles).
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayResult {
    pub data: String,
    pub last_seq: u64,
    pub dropped: bool,
    pub session_id: SessionId,
    pub state: Lifecycle,
}

struct RegistryInner {
    sessions: HashMap<SessionId, Arc<PtySession>>,
    pane_to_session: HashMap<PaneId, SessionId>,
    workspace_to_sessions: HashMap<WorkspaceId, Vec<SessionId>>,
    active_workspace: Option<WorkspaceId>,
}

pub struct SessionRegistry {
    inner: Mutex<RegistryInner>,
    /// Directory for per-session output spool files (M2.2).
    spool_dir: PathBuf,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve the default shell: pwsh > powershell > cmd (spec 7.2).
pub fn detect_shell() -> Result<String, String> {
    for cand in SHELL_CANDIDATES {
        if let Ok(path) = which::which(cand) {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    Err(format!(
        "no usable shell found (tried: {})",
        SHELL_CANDIDATES.join(", ")
    ))
}

/// Track DECSET/DECRST 2004 (bracketed paste) in the output stream.
/// Returns the last state change found in the chunk, or None if absent.
/// Known limitation: a sequence split exactly across a chunk boundary is
/// missed; acceptable because apps re-assert the mode on prompt redraws.
fn scan_bracketed_paste(chunk: &str) -> Option<bool> {
    let on = chunk.rfind("\x1b[?2004h");
    let off = chunk.rfind("\x1b[?2004l");
    match (on, off) {
        (Some(a), Some(b)) => Some(a > b),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectReceipt {
    pub pane_id: PaneId,
    pub session_id: SessionId,
    pub bytes: usize,
    pub bracketed: bool,
    pub submitted: bool,
}

/// Extract the longest valid UTF-8 prefix from `pending`, leaving an
/// incomplete trailing multi-byte sequence in place for the next chunk.
/// Invalid bytes are replaced with U+FFFD so the stream never stalls.
fn extract_utf8(pending: &mut Vec<u8>) -> String {
    let mut out = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(s) => {
                out.push_str(s);
                pending.clear();
                break;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                out.push_str(std::str::from_utf8(&pending[..valid]).expect("validated prefix"));
                match e.error_len() {
                    Some(len) => {
                        out.push('\u{FFFD}');
                        pending.drain(..valid + len);
                    }
                    None => {
                        // Incomplete sequence at the tail: keep for next read.
                        pending.drain(..valid);
                        break;
                    }
                }
            }
        }
    }
    out
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::with_spool_dir(std::env::temp_dir().join("terminal-f-spool"))
    }

    pub fn with_spool_dir(spool_dir: PathBuf) -> Self {
        Self {
            inner: Mutex::new(RegistryInner {
                sessions: HashMap::new(),
                pane_to_session: HashMap::new(),
                workspace_to_sessions: HashMap::new(),
                active_workspace: None,
            }),
            spool_dir,
        }
    }

    pub fn live_count(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner
            .sessions
            .values()
            .filter(|s| {
                matches!(
                    *s.state.lock().unwrap(),
                    Lifecycle::Starting | Lifecycle::Running
                )
            })
            .count()
    }

    pub fn has_session(&self, session_id: &str) -> bool {
        self.inner.lock().unwrap().sessions.contains_key(session_id)
    }

    pub fn session_info(&self, session_id: &str) -> Option<SessionInfo> {
        let inner = self.inner.lock().unwrap();
        inner.sessions.get(session_id).map(|s| SessionInfo {
            pane_id: s.pane_id.clone(),
            session_id: s.session_id.clone(),
            workspace_id: s.workspace_id.clone(),
            state: *s.state.lock().unwrap(),
            exit_code: *s.exit_code.lock().unwrap(),
            command: s.command.clone(),
        })
    }

    /// Spawn a new PTY session for a pane. Refused beyond the live soft cap
    /// (ADR-005 policy: refuse, do not queue).
    pub fn spawn_session(
        &self,
        workspace_id: &str,
        pane_id: &str,
        cwd: &str,
        command: Option<&str>,
    ) -> Result<Arc<PtySession>, String> {
        if self.live_count() >= LIVE_PTY_SOFT_CAP {
            return Err(format!(
                "live PTY soft cap ({LIVE_PTY_SOFT_CAP}) reached; close unused panes first"
            ));
        }

        let program = match command {
            Some(c) if !c.trim().is_empty() => c.to_string(),
            _ => detect_shell()?,
        };

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("openpty failed: {e}"))?;

        let mut cmd = CommandBuilder::new(&program);
        let cwd_path = Path::new(cwd);
        if cwd_path.is_dir() {
            cmd.cwd(cwd_path);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to spawn '{program}': {e}"))?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone PTY reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("failed to take PTY writer: {e}"))?;

        let session_id = new_id();
        let spool_path = self.spool_dir.join(format!("{session_id}.log"));
        let session = Arc::new(PtySession {
            session_id,
            workspace_id: workspace_id.to_string(),
            pane_id: pane_id.to_string(),
            command: program,
            cwd: cwd.to_string(),
            state: Mutex::new(Lifecycle::Running),
            exit_code: Mutex::new(None),
            ring: Mutex::new(RingBuffer::new(RING_MAX_BYTES, RING_MAX_CHUNKS)),
            writer: Mutex::new(Some(writer)),
            master: Mutex::new(Some(pair.master)),
            child: Mutex::new(Some(child)),
            reader_join: Mutex::new(None),
            last_emitted_seq: AtomicU64::new(0),
            // Events start flowing only after the frontend replays this pane,
            // so mount-time snapshot/replay/live ordering is race-free.
            replay_synced: AtomicBool::new(false),
            exit_notified: AtomicBool::new(false),
            last_output_at: Mutex::new(std::time::Instant::now()),
            bracketed_paste: AtomicBool::new(false),
            observe: AtomicBool::new(false),
            spool: Mutex::new(None),
            spool_path,
        });

        let join = spawn_reader_thread(Arc::clone(&session), reader);
        *session.reader_join.lock().unwrap() = Some(join);

        {
            let mut inner = self.inner.lock().unwrap();
            inner
                .sessions
                .insert(session.session_id.clone(), Arc::clone(&session));
            inner
                .pane_to_session
                .insert(pane_id.to_string(), session.session_id.clone());
            inner
                .workspace_to_sessions
                .entry(workspace_id.to_string())
                .or_default()
                .push(session.session_id.clone());
        }
        Ok(session)
    }

    pub fn session_for_pane(&self, pane_id: &str) -> Option<Arc<PtySession>> {
        let inner = self.inner.lock().unwrap();
        let sid = inner.pane_to_session.get(pane_id)?;
        inner.sessions.get(sid).cloned()
    }

    /// Write raw input to the PTY of an explicitly named pane.
    /// stdin injection always requires an explicit pane id (spec 12).
    pub fn write_pane(&self, pane_id: &str, data: &str) -> Result<(), String> {
        let session = self
            .session_for_pane(pane_id)
            .ok_or_else(|| format!("no session for pane {pane_id}"))?;
        if *session.state.lock().unwrap() != Lifecycle::Running {
            return Err(format!("session for pane {pane_id} is not running"));
        }
        let mut guard = session.writer.lock().unwrap();
        let writer = guard
            .as_mut()
            .ok_or_else(|| "PTY writer already closed".to_string())?;
        writer
            .write_all(data.as_bytes())
            .and_then(|_| writer.flush())
            .map_err(|e| format!("PTY write failed: {e}"))
    }

    /// Injection write path (M2.0). Distinct from `write_pane` (the user's
    /// own keystrokes): enforces the idle gate and wraps the payload in
    /// bracketed-paste markers when the target app has enabled them, so a
    /// multi-line prompt cannot self-execute line by line.
    /// Caller (command layer) is responsible for the allow_injection and
    /// pause gates plus audit logging — they live on config state.
    pub fn inject(
        &self,
        pane_id: &str,
        text: &str,
        submit: bool,
        require_idle: bool,
        idle_ms: u64,
    ) -> Result<InjectReceipt, String> {
        let session = self
            .session_for_pane(pane_id)
            .ok_or_else(|| format!("no session for pane {pane_id}"))?;
        if *session.state.lock().unwrap() != Lifecycle::Running {
            return Err(format!("session for pane {pane_id} is not running"));
        }
        if require_idle {
            let idle_for = session.last_output_at.lock().unwrap().elapsed();
            if idle_for < std::time::Duration::from_millis(idle_ms) {
                return Err(format!(
                    "target pane is busy (output {}ms ago; requires {idle_ms}ms of quiet). Retry when idle or pass requireIdle=false",
                    idle_for.as_millis()
                ));
            }
        }
        let bracketed = session.bracketed_paste.load(Ordering::SeqCst);
        let mut payload = String::with_capacity(text.len() + 16);
        if bracketed {
            payload.push_str("\x1b[200~");
        }
        payload.push_str(text);
        if bracketed {
            payload.push_str("\x1b[201~");
        }
        if submit {
            payload.push('\r');
        }
        {
            let mut guard = session.writer.lock().unwrap();
            let writer = guard
                .as_mut()
                .ok_or_else(|| "PTY writer already closed".to_string())?;
            writer
                .write_all(payload.as_bytes())
                .and_then(|_| writer.flush())
                .map_err(|e| format!("PTY write failed: {e}"))?;
        }
        Ok(InjectReceipt {
            pane_id: pane_id.to_string(),
            session_id: session.session_id.clone(),
            bytes: payload.len(),
            bracketed,
            submitted: submit,
        })
    }

    pub fn resize_pane(&self, pane_id: &str, rows: u16, cols: u16) -> Result<(), String> {
        let session = self
            .session_for_pane(pane_id)
            .ok_or_else(|| format!("no session for pane {pane_id}"))?;
        let guard = session.master.lock().unwrap();
        let master = guard
            .as_ref()
            .ok_or_else(|| "PTY already closed".to_string())?;
        master
            .resize(PtySize {
                rows: rows.max(2),
                cols: cols.max(2),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("PTY resize failed: {e}"))
    }

    /// Fetch buffered output after `from_seq` and re-arm live event emission
    /// from the end of the returned range.
    pub fn replay(&self, pane_id: &str, from_seq: u64) -> Result<ReplayResult, String> {
        let session = self
            .session_for_pane(pane_id)
            .ok_or_else(|| format!("no session for pane {pane_id}"))?;
        let (data, last_seq, dropped) = session.ring.lock().unwrap().collect_since(from_seq);
        session.last_emitted_seq.store(last_seq, Ordering::SeqCst);
        session.replay_synced.store(true, Ordering::SeqCst);
        let state = *session.state.lock().unwrap();
        Ok(ReplayResult {
            data,
            last_seq,
            dropped,
            session_id: session.session_id.clone(),
            state,
        })
    }

    /// Run a template `startupCommand` in a freshly spawned session: wait for
    /// the shell to print its prompt and go idle, then type the command. This
    /// is the user's own template action (not external injection), so it is
    /// not gated by `allow_injection` and is not audited as injection.
    pub fn run_startup(&self, pane_id: &str, command: &str) {
        let Some(session) = self.session_for_pane(pane_id) else {
            return;
        };
        let cmd = command.to_string();
        std::thread::Builder::new()
            .name(format!("startup-{pane_id}"))
            .spawn(move || {
                use std::time::{Duration, Instant};
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let has_output = session.ring.lock().unwrap().total_bytes > 0;
                    let idle = session.last_output_at.lock().unwrap().elapsed();
                    if has_output && idle >= Duration::from_millis(800) {
                        break;
                    }
                    if Instant::now() > deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if *session.state.lock().unwrap() == Lifecycle::Running {
                    if let Some(w) = session.writer.lock().unwrap().as_mut() {
                        let _ = w.write_all(cmd.as_bytes());
                        let _ = w.write_all(b"\r");
                        let _ = w.flush();
                    }
                }
            })
            .ok();
    }

    /// Enable/disable output observation (spooling) for a pane's session.
    /// Enabling opens the spool writer if needed; disabling stops appending
    /// but keeps the file so the broker can still read what was captured.
    pub fn set_observe(&self, pane_id: &str, enabled: bool) -> Result<(), String> {
        let session = self
            .session_for_pane(pane_id)
            .ok_or_else(|| format!("no session for pane {pane_id}"))?;
        if enabled {
            let mut guard = session.spool.lock().unwrap();
            if guard.is_none() {
                let w = crate::spool::SpoolWriter::create(
                    &session.spool_path,
                    crate::spool::SPOOL_CAP_BYTES,
                )
                .map_err(|e| format!("spool create failed: {e}"))?;
                *guard = Some(w);
            }
        }
        session.observe.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    /// Read observed output from a pane's spool by byte offset (control API).
    pub fn read_output(
        &self,
        pane_id: &str,
        from: u64,
        max: usize,
    ) -> Result<crate::spool::SpoolRead, String> {
        let session = self
            .session_for_pane(pane_id)
            .ok_or_else(|| format!("no session for pane {pane_id}"))?;
        crate::spool::read_spool(&session.spool_path, from, max)
            .map_err(|e| format!("spool read failed: {e}"))
    }

    /// Switch the active workspace. Sessions of both the previously active
    /// and the newly active workspace stop emitting until the frontend
    /// replays each mounted pane.
    pub fn set_active_workspace(&self, workspace_id: Option<&str>) {
        let mut inner = self.inner.lock().unwrap();
        let mut unsync: Vec<SessionId> = Vec::new();
        if let Some(prev) = inner.active_workspace.clone() {
            if let Some(ids) = inner.workspace_to_sessions.get(&prev) {
                unsync.extend(ids.iter().cloned());
            }
        }
        if let Some(ws) = workspace_id {
            if let Some(ids) = inner.workspace_to_sessions.get(ws) {
                unsync.extend(ids.iter().cloned());
            }
        }
        for sid in unsync {
            if let Some(s) = inner.sessions.get(&sid) {
                s.replay_synced.store(false, Ordering::SeqCst);
            }
        }
        inner.active_workspace = workspace_id.map(String::from);
    }

    pub fn active_workspace(&self) -> Option<WorkspaceId> {
        self.inner.lock().unwrap().active_workspace.clone()
    }

    /// Per-workspace activity summary for sidebar indicators: whether any
    /// session has output the frontend hasn't seen (ring ahead of last
    /// emitted seq) and how many sessions have exited.
    pub fn activity_summary(
        &self,
    ) -> HashMap<WorkspaceId, (bool /* unseen */, usize /* exited */, usize /* live */)> {
        let inner = self.inner.lock().unwrap();
        let mut out: HashMap<WorkspaceId, (bool, usize, usize)> = HashMap::new();
        for (ws, ids) in &inner.workspace_to_sessions {
            let entry = out.entry(ws.clone()).or_insert((false, 0, 0));
            for sid in ids {
                let Some(s) = inner.sessions.get(sid) else {
                    continue;
                };
                let last_emitted = s.last_emitted_seq.load(Ordering::SeqCst);
                if s.ring.lock().unwrap().last_seq() > last_emitted {
                    entry.0 = true;
                }
                match *s.state.lock().unwrap() {
                    Lifecycle::Exited => entry.1 += 1,
                    Lifecycle::Starting | Lifecycle::Running => entry.2 += 1,
                    Lifecycle::Closing => {}
                }
            }
        }
        out
    }

    /// Sessions belonging to the active workspace (for the output emitter).
    pub fn active_sessions_snapshot(&self) -> Vec<Arc<PtySession>> {
        let inner = self.inner.lock().unwrap();
        let Some(active) = inner.active_workspace.clone() else {
            return Vec::new();
        };
        inner
            .workspace_to_sessions
            .get(&active)
            .map(|ids| {
                ids.iter()
                    .filter_map(|sid| inner.sessions.get(sid).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Gracefully terminate the session attached to a pane (pane close path).
    pub fn close_pane_session(&self, pane_id: &str) -> Result<(), String> {
        let session = {
            let mut inner = self.inner.lock().unwrap();
            let Some(sid) = inner.pane_to_session.remove(pane_id) else {
                return Ok(()); // pane had no session; nothing to do
            };
            let session = inner.sessions.remove(&sid);
            if let Some(s) = &session {
                if let Some(ids) = inner.workspace_to_sessions.get_mut(&s.workspace_id) {
                    ids.retain(|x| x != &sid);
                }
            }
            session
        };
        if let Some(session) = session {
            teardown_session(&session);
        }
        Ok(())
    }

    /// Terminate all sessions of a workspace (workspace delete path).
    pub fn close_workspace_sessions(&self, workspace_id: &str) {
        let sessions: Vec<Arc<PtySession>> = {
            let mut inner = self.inner.lock().unwrap();
            let ids = inner
                .workspace_to_sessions
                .remove(workspace_id)
                .unwrap_or_default();
            ids.iter()
                .filter_map(|sid| {
                    let s = inner.sessions.remove(sid);
                    if let Some(sess) = &s {
                        inner.pane_to_session.remove(&sess.pane_id);
                    }
                    s
                })
                .collect()
        };
        for s in sessions {
            teardown_session(&s);
        }
    }

    /// Terminate everything (app exit path).
    pub fn shutdown(&self) {
        let sessions: Vec<Arc<PtySession>> = {
            let mut inner = self.inner.lock().unwrap();
            inner.pane_to_session.clear();
            inner.workspace_to_sessions.clear();
            inner.sessions.drain().map(|(_, s)| s).collect()
        };
        for s in sessions {
            teardown_session(&s);
        }
    }
}

fn spawn_reader_thread(
    session: Arc<PtySession>,
    mut reader: Box<dyn Read + Send>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("pty-reader-{}", &session.pane_id))
        .spawn(move || {
            let mut buf = [0u8; READ_CHUNK_SIZE];
            let mut pending: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&buf[..n]);
                        let text = extract_utf8(&mut pending);
                        if !text.is_empty() {
                            *session.last_output_at.lock().unwrap() = std::time::Instant::now();
                            if let Some(enabled) = scan_bracketed_paste(&text) {
                                session.bracketed_paste.store(enabled, Ordering::SeqCst);
                            }
                            // Observed panes also spool to disk so the control
                            // API can serve full output by byte offset (M2.2).
                            if session.observe.load(Ordering::SeqCst) {
                                if let Some(w) = session.spool.lock().unwrap().as_mut() {
                                    w.append(text.as_bytes());
                                }
                            }
                            // Bounded push: never blocks, drops oldest on overflow,
                            // so a slow/absent UI cannot grow backend memory (ADR-004).
                            session.ring.lock().unwrap().push(text);
                        }
                    }
                }
            }
            let code = {
                let mut child_guard = session.child.lock().unwrap();
                child_guard
                    .as_mut()
                    .and_then(|c| c.wait().ok())
                    .map(|st| st.exit_code())
            };
            *session.exit_code.lock().unwrap() = code;
            let mut state = session.state.lock().unwrap();
            if *state != Lifecycle::Closing {
                *state = Lifecycle::Exited;
            }
        })
        .expect("failed to spawn pty reader thread")
}

/// Kill the child, drop the ConPTY handles (which unblocks the reader at EOF),
/// then join the reader thread.
fn teardown_session(session: &Arc<PtySession>) {
    *session.state.lock().unwrap() = Lifecycle::Closing;
    if let Some(child) = session.child.lock().unwrap().as_mut() {
        let _ = child.kill();
    }
    *session.writer.lock().unwrap() = None;
    *session.master.lock().unwrap() = None;
    if let Some(join) = session.reader_join.lock().unwrap().take() {
        let _ = join.join();
    }
    *session.child.lock().unwrap() = None;
    // Drop the spool writer and remove its file (output history is not
    // persisted across a pane's lifetime).
    *session.spool.lock().unwrap() = None;
    let _ = std::fs::remove_file(&session.spool_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_seq_and_collect() {
        let mut ring = RingBuffer::new(1024, 16);
        assert_eq!(ring.push("a".into()), 1);
        assert_eq!(ring.push("b".into()), 2);
        assert_eq!(ring.push("c".into()), 3);
        let (data, last, dropped) = ring.collect_since(0);
        assert_eq!(data, "abc");
        assert_eq!(last, 3);
        assert!(!dropped);
        let (data, last, dropped) = ring.collect_since(2);
        assert_eq!(data, "c");
        assert_eq!(last, 3);
        assert!(!dropped);
    }

    #[test]
    fn ring_buffer_drops_oldest_and_reports_gap() {
        let mut ring = RingBuffer::new(10, 1024);
        for i in 0..10 {
            ring.push(format!("{i:04}")); // 4 bytes each; budget 10 -> keeps 2
        }
        assert!(ring.dropped_chunks >= 8);
        let (_, last, dropped) = ring.collect_since(0);
        assert_eq!(last, 10);
        assert!(dropped, "gap from seq 0 must be reported");
        let (data, _, dropped2) = ring.collect_since(8);
        assert_eq!(data, "00080009", "seq 9 and 10 are retained");
        assert!(!dropped2, "no gap when reading only retained chunks");
    }

    #[test]
    fn ring_buffer_chunk_cap() {
        let mut ring = RingBuffer::new(usize::MAX, 4);
        for _ in 0..10 {
            ring.push("x".into());
        }
        assert_eq!(ring.dropped_chunks, 6);
        let (data, _, _) = ring.collect_since(0);
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn utf8_boundary_repair() {
        // "한" = EB 8A ... let's use actual bytes of "한글" (UTF-8: ED 95 9C EA B8 80)
        let bytes = "한글".as_bytes();
        let mut pending: Vec<u8> = Vec::new();
        pending.extend_from_slice(&bytes[..4]); // splits the second char
        let first = extract_utf8(&mut pending);
        assert_eq!(first, "한");
        assert_eq!(pending.len(), 1);
        pending.extend_from_slice(&bytes[4..]);
        let second = extract_utf8(&mut pending);
        assert_eq!(second, "글");
        assert!(pending.is_empty());
    }

    #[test]
    fn utf8_invalid_bytes_replaced() {
        let mut pending = vec![b'a', 0xFF, b'b'];
        let out = extract_utf8(&mut pending);
        assert_eq!(out, "a\u{FFFD}b");
        assert!(pending.is_empty());
    }

    #[test]
    fn bracketed_paste_scan() {
        assert_eq!(scan_bracketed_paste("plain output"), None);
        assert_eq!(scan_bracketed_paste("x\x1b[?2004hy"), Some(true));
        assert_eq!(scan_bracketed_paste("x\x1b[?2004ly"), Some(false));
        // last state change wins
        assert_eq!(scan_bracketed_paste("\x1b[?2004h..\x1b[?2004l"), Some(false));
        assert_eq!(scan_bracketed_paste("\x1b[?2004l..\x1b[?2004h"), Some(true));
    }

    #[test]
    fn detect_shell_finds_something_on_windows() {
        // On any Windows box at least cmd must resolve.
        let shell = detect_shell().expect("a shell must be found");
        assert!(!shell.is_empty());
    }
}
