//! Tauri IPC commands. All layout/workspace mutations happen here (backend
//! is the source of truth); the frontend renders whatever these return.
//!
//! Lock ordering: store -> registry. Never take the store lock while holding
//! a registry lock.

use crate::layout;
use crate::model::{now_ms, Direction, Workspace, WorkspaceId};
use crate::session::SessionInfo;
use crate::state::{AppState, WorkspaceMeta};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub workspaces: Vec<WorkspaceMeta>,
    pub active_workspace_id: Option<WorkspaceId>,
    pub ui: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceActivity {
    pub workspace_id: WorkspaceId,
    pub unseen_output: bool,
    pub exited_panes: usize,
    pub live_panes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceResult {
    pub workspace: Workspace,
    pub sessions: Vec<SessionInfo>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceResult {
    pub workspace: Workspace,
    pub warning: Option<String>,
    pub workspaces: Vec<WorkspaceMeta>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootInfo {
    pub autotest: bool,
    pub shell: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub rss_bytes: u64,
    pub live_sessions: usize,
}

fn persist(state: &AppState) {
    let rules = state.automation.lock().unwrap().rules.clone();
    let cfg = state.store.lock().unwrap().to_config(rules);
    if let Err(e) = crate::config::save_config(&state.config_path, &cfg) {
        eprintln!("[terminal-f] config save failed: {e}");
    }
}

/// Spawn sessions for panes of a workspace that don't have a live one yet
/// (lazy spawn, ADR-005). Returns warnings for panes that could not spawn.
fn ensure_sessions(state: &AppState, workspace_id: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut store = state.store.lock().unwrap();
    let Some(ws) = store.get_mut(workspace_id) else {
        return vec![format!("workspace not found: {workspace_id}")];
    };
    for leaf in layout::collect_panes_mut(&mut ws.root) {
        let alive = leaf
            .session_id
            .as_ref()
            .map(|sid| state.registry.has_session(sid))
            .unwrap_or(false);
        if alive {
            continue;
        }
        match state
            .registry
            .spawn_session(workspace_id, &leaf.id, &leaf.cwd, leaf.command.as_deref())
        {
            Ok(session) => {
                leaf.session_id = Some(session.session_id.clone());
                // Re-arm observation spooling for a freshly spawned session.
                if leaf.allow_observe {
                    let _ = state.registry.set_observe(&leaf.id, true);
                }
                // Run the template startup command once the shell is ready.
                if let Some(cmd) = &leaf.startup_command {
                    state.registry.run_startup(&leaf.id, cmd);
                }
            }
            Err(e) => {
                leaf.session_id = None;
                warnings.push(format!("pane {}: {e}", leaf.id));
            }
        }
    }
    warnings
}

fn workspace_sessions(state: &AppState, ws: &Workspace) -> Vec<SessionInfo> {
    layout::collect_panes(&ws.root)
        .iter()
        .filter_map(|leaf| {
            leaf.session_id
                .as_ref()
                .and_then(|sid| state.registry.session_info(sid))
        })
        .collect()
}

fn workspace_result(state: &AppState, workspace_id: &str, warnings: Vec<String>) -> Result<WorkspaceResult, String> {
    let store = state.store.lock().unwrap();
    let ws = store
        .get(workspace_id)
        .ok_or_else(|| format!("workspace not found: {workspace_id}"))?
        .clone();
    drop(store);
    let sessions = workspace_sessions(state, &ws);
    Ok(WorkspaceResult {
        workspace: ws,
        sessions,
        warnings,
    })
}

// ---------------------------------------------------------------- workspace

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> AppSnapshot {
    let store = state.store.lock().unwrap();
    AppSnapshot {
        workspaces: store.metas(),
        active_workspace_id: store.active_workspace_id.clone(),
        ui: store.ui.clone(),
    }
}

/// Sidebar activity indicators (polled ~1/s by the frontend).
#[tauri::command]
pub fn workspace_activity(state: State<'_, AppState>) -> Vec<WorkspaceActivity> {
    let summary = state.registry.activity_summary();
    let store = state.store.lock().unwrap();
    store
        .workspaces
        .iter()
        .map(|w| {
            let (unseen, exited, live) = summary.get(&w.id).copied().unwrap_or((false, 0, 0));
            WorkspaceActivity {
                workspace_id: w.id.clone(),
                unseen_output: unseen,
                exited_panes: exited,
                live_panes: live,
            }
        })
        .collect()
}

#[tauri::command]
pub fn reorder_workspaces(
    state: State<'_, AppState>,
    workspace_ids: Vec<String>,
) -> Result<Vec<WorkspaceMeta>, String> {
    let metas = {
        let mut store = state.store.lock().unwrap();
        store.reorder(&workspace_ids)?;
        store.metas()
    };
    persist(&state);
    Ok(metas)
}

#[tauri::command]
pub fn set_workspace_color(
    state: State<'_, AppState>,
    workspace_id: String,
    color: Option<String>,
) -> Result<Vec<WorkspaceMeta>, String> {
    let metas = {
        let mut store = state.store.lock().unwrap();
        store.set_color(&workspace_id, color)?;
        store.metas()
    };
    persist(&state);
    Ok(metas)
}

/// Replace persisted UI preferences (theme, sidebar state, font).
#[tauri::command]
pub fn set_ui_prefs(state: State<'_, AppState>, ui: serde_json::Value) {
    {
        let mut store = state.store.lock().unwrap();
        store.ui = ui;
    }
    persist(&state);
}

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> Vec<WorkspaceMeta> {
    state.store.lock().unwrap().metas()
}

#[tauri::command]
pub fn create_workspace(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<CreateWorkspaceResult, String> {
    let (workspace, warning, metas) = {
        let mut store = state.store.lock().unwrap();
        let default_name = format!("workspace-{}", store.workspaces.len() + 1);
        let (ws, warning) = store.create(name.as_deref().unwrap_or(&default_name))?;
        (ws, warning, store.metas())
    };
    persist(&state);
    Ok(CreateWorkspaceResult {
        workspace,
        warning,
        workspaces: metas,
    })
}

#[tauri::command]
pub fn rename_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> Result<Vec<WorkspaceMeta>, String> {
    let metas = {
        let mut store = state.store.lock().unwrap();
        store.rename(&workspace_id, &name)?;
        store.metas()
    };
    persist(&state);
    Ok(metas)
}

/// Delete a workspace and gracefully terminate all of its PTY sessions.
#[tauri::command]
pub fn delete_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<AppSnapshot, String> {
    {
        let mut store = state.store.lock().unwrap();
        store.delete(&workspace_id)?;
    }
    state.registry.close_workspace_sessions(&workspace_id);
    // If the deleted workspace was the active one at the registry level,
    // event emission for it is already gone with its sessions; the frontend
    // follows up with switch_workspace(next_active).
    persist(&state);
    let store = state.store.lock().unwrap();
    Ok(AppSnapshot {
        workspaces: store.metas(),
        active_workspace_id: store.active_workspace_id.clone(),
        ui: store.ui.clone(),
    })
}

/// Switch the active workspace. PTY sessions of the previous workspace are
/// NOT terminated (keep-alive, ADR-002); missing sessions of the target
/// workspace are spawned lazily.
#[tauri::command]
pub fn switch_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<WorkspaceResult, String> {
    {
        let mut store = state.store.lock().unwrap();
        if store.get(&workspace_id).is_none() {
            return Err(format!("workspace not found: {workspace_id}"));
        }
        store.active_workspace_id = Some(workspace_id.clone());
    }
    let warnings = ensure_sessions(&state, &workspace_id);
    state.registry.set_active_workspace(Some(&workspace_id));
    persist(&state);
    workspace_result(&state, &workspace_id, warnings)
}

// ---------------------------------------------------------------- pane tree

#[tauri::command]
pub fn split_pane(
    state: State<'_, AppState>,
    workspace_id: String,
    pane_id: String,
    direction: Direction,
) -> Result<WorkspaceResult, String> {
    {
        let mut store = state.store.lock().unwrap();
        let ws = store
            .get_mut(&workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let leaf_cwd = layout::find_pane(&ws.root, &pane_id)
            .map(|l| l.cwd.clone())
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        // Prefer the shell's live cwd (OSC 9;9 shell integration, ADR-011) so a
        // split opens where the user actually is; fall back to the leaf's
        // creation-time cwd when no integration/report exists. Guard on it
        // being a real directory so a stale/garbage report can't break spawn.
        let cwd = state
            .registry
            .pane_live_cwd(&pane_id)
            .filter(|c| std::path::Path::new(c).is_dir())
            .unwrap_or(leaf_cwd);
        let new_leaf = layout::new_pane_leaf(&cwd);
        let new_pane_id = layout::split_pane(&mut ws.root, &pane_id, direction, new_leaf)
            .map_err(|e| e.to_string())?;
        ws.active_pane_id = Some(new_pane_id);
        ws.updated_at = now_ms();
    }
    // Spawn only when the workspace is the active one; inactive workspaces
    // get their sessions lazily on switch (ADR-005).
    let is_active = state.store.lock().unwrap().active_workspace_id.as_deref()
        == Some(workspace_id.as_str());
    let warnings = if is_active {
        ensure_sessions(&state, &workspace_id)
    } else {
        Vec::new()
    };
    persist(&state);
    workspace_result(&state, &workspace_id, warnings)
}

/// Close a pane: remove it from the tree (sibling promotion) and gracefully
/// terminate its PTY session. Closing the last pane is refused.
#[tauri::command]
pub fn close_pane(
    state: State<'_, AppState>,
    workspace_id: String,
    pane_id: String,
) -> Result<WorkspaceResult, String> {
    {
        let mut store = state.store.lock().unwrap();
        let ws = store
            .get_mut(&workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        layout::close_pane(&mut ws.root, &pane_id).map_err(|e| e.to_string())?;
        if ws.active_pane_id.as_deref() == Some(pane_id.as_str()) {
            ws.active_pane_id = Some(layout::first_pane_id(&ws.root));
        }
        ws.updated_at = now_ms();
    }
    state.registry.close_pane_session(&pane_id)?;
    persist(&state);
    workspace_result(&state, &workspace_id, Vec::new())
}

#[tauri::command]
pub fn resize_split(
    state: State<'_, AppState>,
    workspace_id: String,
    split_id: String,
    ratio: f32,
) -> Result<f32, String> {
    let clamped = {
        let mut store = state.store.lock().unwrap();
        let ws = store
            .get_mut(&workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let clamped =
            layout::resize_split(&mut ws.root, &split_id, ratio).map_err(|e| e.to_string())?;
        ws.updated_at = now_ms();
        clamped
    };
    persist(&state);
    Ok(clamped)
}

#[tauri::command]
pub fn set_active_pane(
    state: State<'_, AppState>,
    workspace_id: String,
    pane_id: String,
) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    let ws = store
        .get_mut(&workspace_id)
        .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
    if !layout::contains_pane(&ws.root, &pane_id) {
        return Err(format!("pane not found: {pane_id}"));
    }
    ws.active_pane_id = Some(pane_id);
    Ok(())
}

// ---------------------------------------------------------------- pty io

/// Write raw input to one explicit pane's PTY (spec 12: stdin injection
/// always requires an explicit pane id).
#[tauri::command]
pub fn write_pane(state: State<'_, AppState>, pane_id: String, data: String) -> Result<(), String> {
    state.registry.write_pane(&pane_id, &data)
}

// TODO(M1): broadcast_write(pane_ids, data) — intentionally NOT implemented
// in M0. Multi-pane stdin injection needs an allowlist/confirmation design
// first (spec 7.3 / 12).

// ------------------------------------------------------------ injection (M2.0)

#[tauri::command]
pub fn set_pane_labels(
    state: State<'_, AppState>,
    workspace_id: String,
    pane_id: String,
    labels: Vec<String>,
) -> Result<WorkspaceResult, String> {
    {
        let mut store = state.store.lock().unwrap();
        let ws = store
            .get_mut(&workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let leaf = layout::collect_panes_mut(&mut ws.root)
            .into_iter()
            .find(|l| l.id == pane_id)
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        leaf.labels = labels
            .into_iter()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        ws.updated_at = now_ms();
    }
    persist(&state);
    workspace_result(&state, &workspace_id, Vec::new())
}

/// Per-pane observation opt-in (default off, M2.2). Enabling starts spooling
/// the session's output so the control API can serve it; the terminal content
/// may hold secrets, hence the explicit gate.
#[tauri::command]
pub fn set_pane_observe(
    state: State<'_, AppState>,
    workspace_id: String,
    pane_id: String,
    allow: bool,
) -> Result<WorkspaceResult, String> {
    {
        let mut store = state.store.lock().unwrap();
        let ws = store
            .get_mut(&workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let leaf = layout::collect_panes_mut(&mut ws.root)
            .into_iter()
            .find(|l| l.id == pane_id)
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        leaf.allow_observe = allow;
        ws.updated_at = now_ms();
    }
    // Start/stop live spooling if the pane has a session (ignore if none yet;
    // ensure_sessions re-applies observe on spawn).
    let _ = state.registry.set_observe(&pane_id, allow);
    persist(&state);
    workspace_result(&state, &workspace_id, Vec::new())
}

/// Read observed output from a pane by byte offset (control API + UI/tests).
#[tauri::command]
pub fn read_pane_output(
    state: State<'_, AppState>,
    pane_id: String,
    from: u64,
    max: Option<usize>,
) -> Result<serde_json::Value, String> {
    // Gate: only panes the user marked observable expose their output.
    {
        let store = state.store.lock().unwrap();
        let allowed = store
            .workspaces
            .iter()
            .flat_map(|w| layout::collect_panes(&w.root))
            .find(|l| l.id == pane_id)
            .map(|l| l.allow_observe)
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        if !allowed {
            return Err(format!(
                "pane {pane_id} does not allow observation; enable it first (default off)"
            ));
        }
    }
    let r = state
        .registry
        .read_output(&pane_id, from, max.unwrap_or(64 * 1024).min(1024 * 1024))?;
    Ok(serde_json::json!({
        "data": r.data,
        "offset": r.offset,
        "total": r.total,
    }))
}

/// Per-pane injection opt-in (default off). This is the allowlist gate:
/// no injection path may write to a pane whose flag is false.
#[tauri::command]
pub fn set_pane_injection(
    state: State<'_, AppState>,
    workspace_id: String,
    pane_id: String,
    allow: bool,
) -> Result<WorkspaceResult, String> {
    {
        let mut store = state.store.lock().unwrap();
        let ws = store
            .get_mut(&workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let leaf = layout::collect_panes_mut(&mut ws.root)
            .into_iter()
            .find(|l| l.id == pane_id)
            .ok_or_else(|| format!("pane not found: {pane_id}"))?;
        leaf.allow_injection = allow;
        ws.updated_at = now_ms();
    }
    persist(&state);
    workspace_result(&state, &workspace_id, Vec::new())
}

/// Prompt injection (M2.0: manual trigger only; M2.1 rules reuse this path).
/// Gates, in order: kill switch -> target resolution (explicit paneId or
/// unique label) -> per-pane allowlist -> idle gate (in registry.inject) ->
/// bracketed-paste wrap. Every successful injection is audit-logged.
#[tauri::command]
pub fn inject_prompt(
    state: State<'_, AppState>,
    pane_id: Option<String>,
    label: Option<String>,
    text: String,
    submit: Option<bool>,
    require_idle: Option<bool>,
) -> Result<crate::session::InjectReceipt, String> {
    do_inject(
        &state,
        pane_id.as_deref(),
        label.as_deref(),
        &text,
        submit.unwrap_or(true),
        require_idle.unwrap_or(true),
        "manual",
    )
}

/// Shared injection path for both the manual command and rule-driven fires.
/// Enforces every gate (kill switch -> target resolution -> per-pane allowlist
/// -> idle -> bracketed paste in registry.inject) and audit-logs on success
/// with the given `source` ("manual" or a rule id).
pub fn do_inject(
    state: &AppState,
    pane_id: Option<&str>,
    label: Option<&str>,
    text: &str,
    submit: bool,
    require_idle: bool,
    source: &str,
) -> Result<crate::session::InjectReceipt, String> {
    use std::sync::atomic::Ordering;
    if state.injection_paused.load(Ordering::SeqCst) {
        return Err("injection is paused (kill switch); resume it first".into());
    }
    if text.is_empty() {
        return Err("refusing to inject empty text".into());
    }
    let (workspace_id, target_pane, allowed) = {
        let store = state.store.lock().unwrap();
        crate::state::resolve_inject_target(&store, pane_id, label)?
    };
    if !allowed {
        return Err(format!(
            "pane {target_pane} does not allow injection; enable it on the pane first (allowlist, default off)"
        ));
    }
    let receipt = state.registry.inject(
        &target_pane,
        text,
        submit,
        require_idle,
        crate::session::INJECT_IDLE_MS,
    )?;
    let entry = crate::audit::AuditEntry {
        ts: now_ms(),
        source: source.to_string(),
        workspace_id,
        pane_id: receipt.pane_id.clone(),
        session_id: receipt.session_id.clone(),
        bytes: receipt.bytes,
        submitted: receipt.submitted,
        bracketed: receipt.bracketed,
        preview: crate::audit::preview_of(text),
    };
    if let Err(e) = crate::audit::append(&audit_path(state), &entry) {
        eprintln!("[terminal-f] audit append failed: {e}");
    }
    Ok(receipt)
}

fn audit_path(state: &AppState) -> std::path::PathBuf {
    state.config_path.with_file_name("audit.log")
}

#[tauri::command]
pub fn set_injection_paused(state: State<'_, AppState>, paused: bool) -> bool {
    state
        .injection_paused
        .store(paused, std::sync::atomic::Ordering::SeqCst);
    paused
}

#[tauri::command]
pub fn injection_status(state: State<'_, AppState>) -> bool {
    state
        .injection_paused
        .load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
pub fn read_audit(state: State<'_, AppState>, limit: Option<usize>) -> Vec<crate::audit::AuditEntry> {
    crate::audit::read_tail(&audit_path(&state), limit.unwrap_or(50).min(500))
}

// ---------------------------------------------------- automation (M2.1)

use crate::automation::{Decision, GitSummary, Proposal, Rule};

#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> Vec<Rule> {
    state.automation.lock().unwrap().rules.clone()
}

/// Add or replace (by id) an automation rule.
#[tauri::command]
pub fn upsert_rule(state: State<'_, AppState>, rule: Rule) -> Result<Vec<Rule>, String> {
    rule.validate()?;
    {
        let mut auto = state.automation.lock().unwrap();
        if let Some(existing) = auto.rules.iter_mut().find(|r| r.id == rule.id) {
            *existing = rule;
        } else {
            auto.rules.push(rule);
        }
    }
    persist(&state);
    Ok(state.automation.lock().unwrap().rules.clone())
}

#[tauri::command]
pub fn remove_rule(state: State<'_, AppState>, rule_id: String) -> Vec<Rule> {
    {
        let mut auto = state.automation.lock().unwrap();
        auto.rules.retain(|r| r.id != rule_id);
        auto.gc();
    }
    persist(&state);
    state.automation.lock().unwrap().rules.clone()
}

#[tauri::command]
pub fn set_rule_enabled(
    state: State<'_, AppState>,
    rule_id: String,
    enabled: bool,
) -> Result<Vec<Rule>, String> {
    {
        let mut auto = state.automation.lock().unwrap();
        let rule = auto
            .rules
            .iter_mut()
            .find(|r| r.id == rule_id)
            .ok_or_else(|| format!("rule not found: {rule_id}"))?;
        rule.enabled = enabled;
    }
    persist(&state);
    Ok(state.automation.lock().unwrap().rules.clone())
}

/// Compute a working-tree summary via the `git` CLI. Returns None when the
/// path is not a git repo or git is unavailable (rule simply won't fire).
pub fn git_summary(repo: &str) -> Option<GitSummary> {
    let run = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .ok()?;
        if out.status.success() {
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            None
        }
    };
    let porcelain = run(&["status", "--porcelain"])?;
    let stat = run(&["diff", "--stat"]).unwrap_or_default();
    Some(crate::automation::build_summary(&stat, &porcelain))
}

/// Pending proposal awaiting user approval (confirm mode).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalList {
    pub proposals: Vec<Proposal>,
}

#[tauri::command]
pub fn list_proposals(state: State<'_, AppState>) -> Vec<Proposal> {
    state
        .automation
        .lock()
        .unwrap()
        .pending
        .values()
        .cloned()
        .collect()
}

/// Approve or dismiss a pending proposal. Approving runs the shared injection
/// path (source = rule id); dismissing just drops it.
#[tauri::command]
pub fn resolve_proposal(
    state: State<'_, AppState>,
    proposal_id: String,
    approve: bool,
) -> Result<Option<crate::session::InjectReceipt>, String> {
    let proposal = state
        .automation
        .lock()
        .unwrap()
        .pending
        .remove(&proposal_id);
    let Some(p) = proposal else {
        return Err(format!("proposal not found (already handled?): {proposal_id}"));
    };
    if !approve {
        return Ok(None);
    }
    match do_inject(
        &state,
        p.target_pane.as_deref(),
        p.target_label.as_deref(),
        &p.text,
        p.submit,
        p.require_idle,
        &p.rule_id,
    ) {
        Ok(receipt) => Ok(Some(receipt)),
        Err(e) => {
            // Re-queue so the user can retry (e.g. target was momentarily busy).
            state.automation.lock().unwrap().pending.insert(p.id.clone(), p);
            Err(e)
        }
    }
}

/// Run a rule immediately, ignoring debounce/cooldown/dedup/rate-limit
/// (manual "fire now"). Still honors confirm/auto, allowlist, and idle gates.
#[tauri::command]
pub fn run_rule_now(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<String, String> {
    let rule = state
        .automation
        .lock()
        .unwrap()
        .rules
        .iter()
        .find(|r| r.id == rule_id)
        .cloned()
        .ok_or_else(|| format!("rule not found: {rule_id}"))?;
    // Only git-diff rules have a summary to render; timer rules pass None.
    let summary = match rule.effective_source() {
        crate::automation::RuleSource::GitDiff { repo } => git_summary(&repo),
        crate::automation::RuleSource::Timer { .. } => None,
    };
    fire_rule(&app, &state, &rule, summary.as_ref(), true)
}

/// Resolve target, render text, and either emit a proposal (confirm) or inject
/// directly (auto). Returns a human-readable status string.
fn fire_rule(
    app: &tauri::AppHandle,
    state: &AppState,
    rule: &Rule,
    summary: Option<&GitSummary>,
    manual: bool,
) -> Result<String, String> {
    use tauri::Emitter;
    let empty = GitSummary {
        stat: "(no git changes detected)".into(),
        files: Vec::new(),
        hash: String::new(),
        changed: false,
    };
    let s = summary.unwrap_or(&empty);
    let text = crate::automation::render_template(&rule.template, s);

    // Verify the target is resolvable and injection-enabled before proposing,
    // so we never surface a proposal that cannot possibly be approved.
    {
        let store = state.store.lock().unwrap();
        let (_, _, allowed) = crate::state::resolve_inject_target(
            &store,
            rule.target_pane.as_deref(),
            rule.target_label.as_deref(),
        )?;
        if !allowed {
            return Err(format!(
                "rule '{}': target pane does not allow injection (enable it first)",
                rule.name
            ));
        }
    }

    match rule.mode {
        crate::automation::RuleMode::Auto => {
            let receipt = do_inject(
                state,
                rule.target_pane.as_deref(),
                rule.target_label.as_deref(),
                &text,
                rule.submit,
                rule.require_idle,
                &rule.id,
            )?;
            Ok(format!(
                "rule '{}' injected {} bytes (auto)",
                rule.name, receipt.bytes
            ))
        }
        crate::automation::RuleMode::Confirm => {
            let proposal = Proposal {
                id: crate::model::new_id(),
                rule_id: rule.id.clone(),
                rule_name: rule.name.clone(),
                target_label: rule.target_label.clone(),
                target_pane: rule.target_pane.clone(),
                text,
                submit: rule.submit,
                require_idle: rule.require_idle,
                summary: s.stat.clone(),
            };
            let id = proposal.id.clone();
            state
                .automation
                .lock()
                .unwrap()
                .pending
                .insert(id.clone(), proposal.clone());
            let _ = app.emit("automation-proposal", &proposal);
            Ok(format!(
                "rule '{}' proposed injection {}{}",
                rule.name,
                &id[..8],
                if manual { " (run-now)" } else { "" }
            ))
        }
    }
}

// ---------------------------------------------------- templates (Phase B)

use crate::template::{self, Template};
use std::collections::HashMap;

fn templates_dir(state: &AppState) -> std::path::PathBuf {
    state.config_path.with_file_name("templates")
}

/// Apply a template into a NEW workspace (never overwrites an existing one).
/// Variables in the template are substituted from `params`; panes with a
/// `startupCommand` run it once their shell is ready (after switch/spawn).
#[tauri::command]
pub fn apply_template(
    state: State<'_, AppState>,
    template: Template,
    params: Option<HashMap<String, String>>,
) -> Result<AppSnapshot, String> {
    let params = params.unwrap_or_default();
    template::validate(&template, &params)?;
    let root = template::build_tree(&template, &params);
    layout::check_invariants(&root).map_err(|e| format!("template produced invalid layout: {e}"))?;
    {
        let mut store = state.store.lock().unwrap();
        let (_ws, _warn) = store.create_with_root(&template.name, root)?;
    }
    persist(&state);
    let store = state.store.lock().unwrap();
    Ok(AppSnapshot {
        workspaces: store.metas(),
        active_workspace_id: store.active_workspace_id.clone(),
        ui: store.ui.clone(),
    })
}

#[tauri::command]
pub fn list_templates(state: State<'_, AppState>) -> Vec<String> {
    let dir = templates_dir(&state);
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

#[tauri::command]
pub fn get_template(state: State<'_, AppState>, name: String) -> Result<Template, String> {
    let path = templates_dir(&state).join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("template read error: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("template parse error: {e}"))
}

#[tauri::command]
pub fn save_template(state: State<'_, AppState>, template: Template) -> Result<Vec<String>, String> {
    if template.name.trim().is_empty() {
        return Err("template name is empty".into());
    }
    // Reject path separators in the name (write stays inside templates dir).
    if template.name.contains(['/', '\\', ':']) {
        return Err("template name must not contain path separators".into());
    }
    let dir = templates_dir(&state);
    std::fs::create_dir_all(&dir).map_err(|e| format!("templates dir error: {e}"))?;
    let path = dir.join(format!("{}.json", template.name));
    let json = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("template write error: {e}"))?;
    Ok(list_templates(state))
}

#[tauri::command]
pub fn delete_template(state: State<'_, AppState>, name: String) -> Vec<String> {
    let path = templates_dir(&state).join(format!("{name}.json"));
    let _ = std::fs::remove_file(path);
    list_templates(state)
}

/// Build a template blueprint from a workspace's current layout (Save as
/// template). The frontend supplies the name; cwds are kept literally.
#[tauri::command]
pub fn workspace_as_template(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> Result<Template, String> {
    let store = state.store.lock().unwrap();
    let ws = store
        .get(&workspace_id)
        .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
    Ok(Template {
        name,
        params: Vec::new(),
        root: template::from_pane_tree(&ws.root),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoProfile {
    pub template: Template,
    /// Whether the profile runs any commands (trust gate applies).
    pub has_commands: bool,
    pub trusted: bool,
    pub repo: String,
}

/// Read a repo-local `.terminal-f/profile.json` template and report whether
/// it runs commands and whether the repo is already trusted (Phase B trust).
#[tauri::command]
pub fn read_repo_profile(state: State<'_, AppState>, repo: String) -> Result<RepoProfile, String> {
    let path = std::path::Path::new(&repo).join(".terminal-f").join("profile.json");
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("no profile at {}: {e}", path.display()))?;
    let template: Template =
        serde_json::from_str(&raw).map_err(|e| format!("profile parse error: {e}"))?;
    let trusted = state.store.lock().unwrap().is_repo_trusted(&repo);
    Ok(RepoProfile {
        has_commands: template::has_commands(&template.root),
        template,
        trusted,
        repo,
    })
}

#[tauri::command]
pub fn trust_repo(state: State<'_, AppState>, repo: String) {
    state.store.lock().unwrap().trust_repo(&repo);
    persist(&state);
}

// ---------------------------------------------------- image paste (ADR-010)

/// Persist a pasted clipboard image (base64 from the frontend paste event)
/// and return its path, which the frontend then pastes into the pane like a
/// dropped file. Bridges Windows terminals dropping image pastes (Claude
/// Code screenshots).
#[tauri::command]
pub fn save_pasted_image(
    state: State<'_, AppState>,
    data_base64: String,
    mime: Option<String>,
) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("pasted image decode error: {e}"))?;
    let dir = state.config_path.with_file_name("paste");
    let ext = crate::paste::ext_for_mime(mime.as_deref().unwrap_or("image/png"));
    let path = crate::paste::save_paste_bytes(&dir, &bytes, ext, crate::model::now_ms())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Read the OS clipboard directly for the Ctrl+V bridge. xterm intercepts the
/// Ctrl+V keydown (so no browser paste event ever fires); the frontend calls
/// this instead: text pastes as text, an image is persisted and pastes as a
/// file path.
#[tauri::command]
pub fn paste_clipboard(state: State<'_, AppState>) -> Result<crate::paste::PasteContent, String> {
    let dir = state.config_path.with_file_name("paste");
    crate::paste::read_clipboard(&dir, crate::model::now_ms())
}

/// Copy selected terminal text to the OS clipboard (smart Ctrl+C / Ctrl+Shift+C
/// / right-click copy). Writing in Rust via arboard sidesteps WebView clipboard
/// permission/focus issues, mirroring paste_clipboard. Empty text is a no-op so
/// a stray copy of nothing never clobbers the clipboard.
#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    crate::paste::write_clipboard_text(&text)
}

// ---------------------------------------------------- open external URL (links)

/// Whether `url` is safe to hand to the OS opener. The URL originates from
/// terminal output (untrusted) via the web-links addon, so we allow ONLY
/// http/https and reject everything else (`javascript:`, `file:`, `data:`, …),
/// plus any control character or whitespace (defense in depth; the opener uses
/// ShellExecute, not a shell, so query-string `&` is fine). Pure so it is unit
/// tested.
pub fn is_safe_external_url(url: &str) -> bool {
    if url.is_empty() || url.len() > 4096 {
        return false;
    }
    if url.chars().any(|c| c.is_control() || c == ' ') {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Open an http/https URL from terminal output in the user's default browser.
/// Ctrl+click on a linkified URL (frontend web-links addon) routes here. We
/// validate the scheme, then delegate to tauri-plugin-opener (ShellExecute on
/// Windows) — never a shell, so there is no argument-injection surface.
#[tauri::command]
pub fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    if !is_safe_external_url(&url) {
        return Err("refused: only http(s) URLs may be opened".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

// ------------------------------------------ shell integration: pwsh $PROFILE

/// Status of an opt-in pwsh `$PROFILE` block (see shellint.rs). Returned for the
/// confirmation UI before we touch the user's profile.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PwshIntegrationInfo {
    /// Resolved pwsh `$PROFILE` path (may not exist on disk yet).
    pub profile_path: Option<String>,
    /// Whether this feature's fenced block is already present.
    pub installed: bool,
    /// Whether the installed block byte-matches the *current* snippet. False
    /// when an older version of the block is present and a refresh would change
    /// it — the UI offers an update in that case. Always true when not installed
    /// is irrelevant; only meaningful together with `installed`.
    pub up_to_date: bool,
    /// The exact block we would add — shown to the user for confirmation.
    pub snippet: String,
    /// Whether a PowerShell binary was found at all.
    pub available: bool,
}

/// The two shell-integration features, each a fenced `$PROFILE` block.
fn feature_blocks(feature: &str) -> Result<(String, &'static str, &'static str), String> {
    match feature {
        "multiline" => Ok((
            crate::shellint::multiline_snippet(),
            crate::shellint::MULTILINE_BEGIN,
            crate::shellint::MULTILINE_END,
        )),
        "cwd" => Ok((
            crate::shellint::cwd_snippet(),
            crate::shellint::CWD_BEGIN,
            crate::shellint::CWD_END,
        )),
        other => Err(format!("unknown shell-integration feature: {other}")),
    }
}

fn resolve_pwsh() -> Option<std::path::PathBuf> {
    which::which("pwsh")
        .or_else(|_| which::which("powershell"))
        .ok()
}

/// Ask pwsh itself for the current-user/current-host profile path (robust
/// against OneDrive Documents redirection etc.).
fn pwsh_profile_path(pwsh: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let out = std::process::Command::new(pwsh)
        .args([
            "-NoProfile",
            "-NoLogo",
            "-NonInteractive",
            "-Command",
            "$PROFILE.CurrentUserCurrentHost",
        ])
        .output()
        .map_err(|e| format!("could not run pwsh: {e}"))?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return Err("pwsh returned an empty $PROFILE path".into());
    }
    Ok(std::path::PathBuf::from(path))
}

/// `$PROFILE` path resolution spawns a pwsh subprocess, whose cold start can be
/// 1s+ on a fresh machine — and the status query runs before the confirm dialog
/// appears, so a naive re-spawn per click makes the menu feel unresponsive
/// (and install would spawn a second time). The path is stable for the app's
/// lifetime, so resolve once and cache it. Keyed by pwsh path so a PATH change
/// (rare) still re-resolves.
static PROFILE_PATH_CACHE: std::sync::Mutex<Option<(std::path::PathBuf, std::path::PathBuf)>> =
    std::sync::Mutex::new(None);

fn cached_profile_path(pwsh: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if let Some((cached_pwsh, cached_profile)) = PROFILE_PATH_CACHE.lock().unwrap().as_ref() {
        if cached_pwsh == pwsh {
            return Ok(cached_profile.clone());
        }
    }
    let profile = pwsh_profile_path(pwsh)?;
    *PROFILE_PATH_CACHE.lock().unwrap() = Some((pwsh.to_path_buf(), profile.clone()));
    Ok(profile)
}

/// Report whether a shell-integration block is installed, without modifying
/// anything. Drives the confirmation dialog.
#[tauri::command]
pub fn pwsh_integration_status(feature: String) -> Result<PwshIntegrationInfo, String> {
    let (snippet, begin, _end) = feature_blocks(&feature)?;
    let Some(pwsh) = resolve_pwsh() else {
        return Ok(PwshIntegrationInfo {
            profile_path: None,
            installed: false,
            up_to_date: false,
            snippet,
            available: false,
        });
    };
    let path = cached_profile_path(&pwsh)?;
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let installed = crate::shellint::is_installed(&existing, begin);
    // Up to date iff the current block is already present verbatim (a refresh
    // would be a no-op). A stale/older block reports installed=true but
    // up_to_date=false so the UI can offer an update.
    let up_to_date = installed && existing.contains(snippet.trim_end());
    Ok(PwshIntegrationInfo {
        profile_path: Some(path.to_string_lossy().into_owned()),
        installed,
        up_to_date,
        snippet,
        available: true,
    })
}

/// Append (or refresh) a shell-integration block in the user's pwsh `$PROFILE`
/// (idempotent). The frontend MUST show the snippet and get explicit
/// confirmation first — this edits a file the user owns.
#[tauri::command]
pub fn install_pwsh_integration(feature: String) -> Result<PwshIntegrationInfo, String> {
    let (snippet, begin, end) = feature_blocks(&feature)?;
    let Some(pwsh) = resolve_pwsh() else {
        return Err("PowerShell (pwsh/powershell) not found on PATH".into());
    };
    let path = cached_profile_path(&pwsh)?;
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    // Upgrade in place: strip any prior (possibly older) block, then append the
    // current one. Idempotent when already up to date; refreshes an old block.
    let updated = crate::shellint::with_block(
        &crate::shellint::without_block(&existing, begin, end),
        &snippet,
        begin,
    );
    if updated != existing {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("profile dir error: {e}"))?;
        }
        std::fs::write(&path, updated).map_err(|e| format!("profile write error: {e}"))?;
    }
    Ok(PwshIntegrationInfo {
        profile_path: Some(path.to_string_lossy().into_owned()),
        installed: true,
        up_to_date: true,
        snippet,
        available: true,
    })
}

// ---------------------------------------------------- control API (M2.2)

/// Route an authenticated control-API method to backend capabilities. Called
/// by the pipe server per request; every capability reuses the same gates as
/// the UI (do_inject, allow_observe), so the pipe cannot bypass them.
pub fn handle_pipe_method(
    app: &tauri::AppHandle,
    method: &str,
    params: &serde_json::Value,
    conn: &crate::pipe::ConnState,
) -> Result<serde_json::Value, String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let s = |k: &str| params.get(k).and_then(|v| v.as_str()).map(String::from);
    match method {
        "listWorkspaces" => {
            let store = state.store.lock().unwrap();
            Ok(serde_json::to_value(store.metas()).unwrap_or_default())
        }
        "listPanes" => {
            let store = state.store.lock().unwrap();
            let panes: Vec<serde_json::Value> = store
                .workspaces
                .iter()
                .flat_map(|w| {
                    layout::collect_panes(&w.root).into_iter().map(move |l| {
                        serde_json::json!({
                            "workspaceId": w.id,
                            "workspaceName": w.name,
                            "paneId": l.id,
                            "labels": l.labels,
                            "allowInjection": l.allow_injection,
                            "allowObserve": l.allow_observe,
                        })
                    })
                })
                .collect();
            Ok(serde_json::json!({ "panes": panes }))
        }
        "readOutput" => {
            let pane_id = resolve_pane_arg(&state, params)?;
            let from = params.get("from").and_then(|v| v.as_u64()).unwrap_or(0);
            let max = params
                .get("max")
                .and_then(|v| v.as_u64())
                .unwrap_or(64 * 1024)
                .min(1024 * 1024) as usize;
            // reuse the gated command path
            read_pane_output(state.clone(), pane_id, from, Some(max))
        }
        "injectPrompt" => {
            let text = s("text").ok_or("injectPrompt requires text")?;
            let submit = params.get("submit").and_then(|v| v.as_bool()).unwrap_or(true);
            let require_idle = params
                .get("requireIdle")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let source = format!("pipe:{}", conn.client);
            let receipt = do_inject(
                &state,
                s("paneId").as_deref(),
                s("label").as_deref(),
                &text,
                submit,
                require_idle,
                &source,
            )?;
            Ok(serde_json::to_value(receipt).unwrap_or_default())
        }
        "listRules" => Ok(serde_json::to_value(
            state.automation.lock().unwrap().rules.clone(),
        )
        .unwrap_or_default()),
        "runRule" => {
            let rule_id = s("ruleId").ok_or("runRule requires ruleId")?;
            let msg = run_rule_now(app.clone(), state.clone(), rule_id)?;
            Ok(serde_json::json!({ "status": msg }))
        }
        other => Err(format!("unknown method: {other}")),
    }
}

/// Resolve a `paneId` or unique `label` param to a pane id for readOutput.
fn resolve_pane_arg(
    state: &AppState,
    params: &serde_json::Value,
) -> Result<String, String> {
    let pane_id = params.get("paneId").and_then(|v| v.as_str());
    let label = params.get("label").and_then(|v| v.as_str());
    let store = state.store.lock().unwrap();
    let (_, pane, _) = crate::state::resolve_inject_target(&store, pane_id, label)?;
    Ok(pane)
}

/// Path where the control-API connection info (pipe name + token) is written.
pub fn control_api_info_path(state: &AppState) -> std::path::PathBuf {
    state.config_path.with_file_name("control-api.json")
}

#[tauri::command]
pub fn control_api_info(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let path = control_api_info_path(&state);
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("control API not started or info unreadable: {e}"))?;
    let info: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("control-api.json parse error: {e}"))?;
    // Return the pipe name and the info-file path, NOT the token (avoid
    // surfacing the secret in the UI); brokers read the file directly.
    Ok(serde_json::json!({
        "pipeName": info.get("pipeName"),
        "infoPath": path.to_string_lossy(),
    }))
}

/// One automation poll pass over all enabled rules. Called by the background
/// thread every POLL_INTERVAL_MS. Kept out of the thread body so it stays
/// simple; git IO happens here, decision logic is in automation::RuleRuntime.
pub fn poll_automation(app: &tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    use tauri::Manager;
    let state = app.state::<AppState>();
    // While globally paused, don't even evaluate (avoids proposal buildup).
    if state.injection_paused.load(Ordering::SeqCst) {
        return;
    }
    let rules: Vec<Rule> = {
        let auto = state.automation.lock().unwrap();
        auto.rules.iter().filter(|r| r.enabled).cloned().collect()
    };
    let now = std::time::Instant::now();
    for rule in rules {
        // Per-source: git-diff derives a change hash; timer uses elapsed time.
        let (summary, hash, decision) = match rule.effective_source() {
            crate::automation::RuleSource::GitDiff { repo } => {
                let summary = git_summary(&repo);
                let hash = summary.as_ref().map(|s| s.hash.clone()).unwrap_or_default();
                let decision = {
                    let mut auto = state.automation.lock().unwrap();
                    auto.runtime_mut(&rule.id)
                        .decide(&rule, Some(hash.as_str()), now, false)
                };
                (summary, Some(hash), decision)
            }
            crate::automation::RuleSource::Timer { every_ms } => {
                let decision = {
                    let mut auto = state.automation.lock().unwrap();
                    auto.runtime_mut(&rule.id).decide_timer(&rule, every_ms, now, false)
                };
                (None, None, decision)
            }
        };
        if let Decision::Fire = decision {
            match fire_rule(app, &state, &rule, summary.as_ref(), false) {
                Ok(msg) => {
                    // Record the fire so we don't refire the same diff / so the
                    // timer interval restarts from this fire.
                    state
                        .automation
                        .lock()
                        .unwrap()
                        .runtime_mut(&rule.id)
                        .record_fire(hash.as_deref(), now);
                    println!("[terminal-f][automation] {msg}");
                }
                Err(e) => {
                    // Do NOT record: target not ready/allowed -> retry next poll.
                    eprintln!("[terminal-f][automation] rule '{}' skipped: {e}", rule.name);
                }
            }
        }
    }
}


#[tauri::command]
pub fn resize_pty(
    state: State<'_, AppState>,
    pane_id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    state.registry.resize_pane(&pane_id, rows, cols)
}

#[tauri::command]
pub fn replay_pane(
    state: State<'_, AppState>,
    pane_id: String,
    from_seq: u64,
) -> Result<crate::session::ReplayResult, String> {
    state.registry.replay(&pane_id, from_seq)
}

// ---------------------------------------------------------------- misc

#[tauri::command]
pub fn get_boot_info() -> BootInfo {
    BootInfo {
        autotest: std::env::var("TERMF_AUTOTEST").map(|v| v == "1").unwrap_or(false),
        shell: crate::session::detect_shell().ok(),
    }
}

#[tauri::command]
pub fn memory_stats(state: State<'_, AppState>) -> MemoryStats {
    MemoryStats {
        rss_bytes: crate::current_rss_bytes(),
        live_sessions: state.registry.live_count(),
    }
}

/// Autotest support: persist the frontend-produced report to disk so the
/// harness can read it after the app exits.
#[tauri::command]
pub fn autotest_report(report: serde_json::Value) -> Result<String, String> {
    let path = std::env::var("TERMF_REPORT_PATH")
        .unwrap_or_else(|_| "autotest-report.json".to_string());
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("report write failed: {e}"))?;
    Ok(path)
}

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle, state: State<'_, AppState>, code: i32) {
    state.registry.shutdown();
    app.exit(code);
}

#[cfg(test)]
mod tests {
    use super::is_safe_external_url;

    #[test]
    fn accepts_http_and_https() {
        assert!(is_safe_external_url("http://x"));
        assert!(is_safe_external_url("https://a.com/p?q=1&r=2")); // query & is fine
        assert!(is_safe_external_url("https://h:8080/p#frag"));
        assert!(is_safe_external_url("HTTPS://X")); // scheme is case-insensitive
        assert!(is_safe_external_url("https://xn--bcher-kva.example/path")); // punycode
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(!is_safe_external_url("javascript:alert(1)"));
        assert!(!is_safe_external_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_safe_external_url("ftp://host/x"));
        assert!(!is_safe_external_url("data:text/html,<script>1</script>"));
        assert!(!is_safe_external_url("vscode://x"));
        assert!(!is_safe_external_url("//evil.com")); // scheme-relative
    }

    #[test]
    fn rejects_empty_whitespace_control_and_overlong() {
        assert!(!is_safe_external_url(""));
        assert!(!is_safe_external_url("http://a b")); // embedded space
        assert!(!is_safe_external_url("http://a\nb")); // newline / control
        assert!(!is_safe_external_url("http://a\tb"));
        assert!(!is_safe_external_url("http://a\x07b")); // BEL
        let long = format!("https://x/{}", "a".repeat(5000));
        assert!(!is_safe_external_url(&long));
    }
}
