//! Workspace store: the backend-owned source of truth for workspace and
//! pane-layout state. The frontend never mutates layout directly; every
//! mutation goes through a Tauri command that operates on this store.

use crate::layout;
use crate::model::{
    default_cwd, new_id, normalize_root_dir, now_ms, Config, PaneNode, Workspace, WorkspaceId,
    CONFIG_SCHEMA_VERSION,
};
use crate::session::SessionRegistry;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const WORKSPACE_SOFT_CAP: usize = 8;
pub const WORKSPACE_HARD_CAP: usize = 16;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMeta {
    pub id: WorkspaceId,
    pub name: String,
    pub color: Option<String>,
    /// 워크스페이스 시작 폴더 (SPEC-WORKSPACE-ROOT-001 §A.4). 컨텍스트 메뉴가
    /// 비활성 워크스페이스를 포함한 모든 우클릭 대상의 현재 경로를 렌더링하려면
    /// metas에 이 값이 필요하다 (R11). `color`와 동일하게 항상 키를 내보낸다
    /// (미설정 시 null). **파이프 경계에서는 제거된다** — commands.rs
    /// `pipe_list_workspaces_value` 참조 (§A.4.2, AC-12).
    pub root_dir: Option<String>,
}

pub struct WorkspaceStore {
    pub workspaces: Vec<Workspace>,
    pub active_workspace_id: Option<WorkspaceId>,
    pub ui: serde_json::Value,
    pub trusted_repos: Vec<String>,
}

impl WorkspaceStore {
    pub fn with_default() -> Self {
        let ws = new_workspace("default");
        Self {
            active_workspace_id: Some(ws.id.clone()),
            workspaces: vec![ws],
            ui: serde_json::json!({}),
            trusted_repos: Vec::new(),
        }
    }

    pub fn from_config(cfg: Config) -> Self {
        let mut store = Self {
            workspaces: cfg.workspaces,
            active_workspace_id: cfg.active_workspace_id,
            ui: cfg.ui,
            trusted_repos: cfg.trusted_repos,
        };
        if store.workspaces.is_empty() {
            store.workspaces.push(new_workspace("default"));
        }
        let valid_active = store
            .active_workspace_id
            .as_ref()
            .map(|id| store.workspaces.iter().any(|w| &w.id == id))
            .unwrap_or(false);
        if !valid_active {
            store.active_workspace_id = Some(store.workspaces[0].id.clone());
        }
        // Repair stale active pane ids defensively.
        for ws in &mut store.workspaces {
            let valid_pane = ws
                .active_pane_id
                .as_ref()
                .map(|p| layout::contains_pane(&ws.root, p))
                .unwrap_or(false);
            if !valid_pane {
                ws.active_pane_id = Some(layout::first_pane_id(&ws.root));
            }
        }
        store
    }

    /// Build a config for persistence. Automation rules live in
    /// `AutomationState`, so the caller passes them in (commands::persist).
    pub fn to_config(&self, automation: Vec<crate::automation::Rule>) -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            active_workspace_id: self.active_workspace_id.clone(),
            workspaces: self.workspaces.clone(),
            ui: self.ui.clone(),
            automation,
            trusted_repos: self.trusted_repos.clone(),
        }
    }

    pub fn is_repo_trusted(&self, repo: &str) -> bool {
        self.trusted_repos.iter().any(|r| r == repo)
    }

    pub fn trust_repo(&mut self, repo: &str) {
        if !self.is_repo_trusted(repo) {
            self.trusted_repos.push(repo.to_string());
        }
    }

    pub fn metas(&self) -> Vec<WorkspaceMeta> {
        self.workspaces
            .iter()
            .map(|w| WorkspaceMeta {
                id: w.id.clone(),
                name: w.name.clone(),
                color: w.color.clone(),
                root_dir: w.root_dir.clone(),
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    /// Create a workspace. Enforces caps: soft cap 8 -> warning, hard cap 16
    /// -> refused (ADR-005).
    pub fn create(&mut self, name: &str) -> Result<(Workspace, Option<String>), String> {
        if self.workspaces.len() >= WORKSPACE_HARD_CAP {
            return Err(format!(
                "workspace hard cap ({WORKSPACE_HARD_CAP}) reached; delete a workspace first"
            ));
        }
        let warning = if self.workspaces.len() >= WORKSPACE_SOFT_CAP {
            Some(format!(
                "workspace soft cap ({WORKSPACE_SOFT_CAP}) exceeded; sessions are spawned lazily but consider closing unused workspaces"
            ))
        } else {
            None
        };
        let ws = new_workspace(name);
        self.workspaces.push(ws.clone());
        Ok((ws, warning))
    }

    /// Reorder workspaces to match `ids`, which must be a permutation of the
    /// current workspace id set (sidebar drag reorder).
    pub fn reorder(&mut self, ids: &[String]) -> Result<(), String> {
        if ids.len() != self.workspaces.len() {
            return Err("reorder list length mismatch".into());
        }
        // Validate before mutating so a bad request can never drop workspaces.
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        if unique.len() != ids.len() {
            return Err("reorder list contains duplicate ids".into());
        }
        for id in ids {
            if !self.workspaces.iter().any(|w| &w.id == id) {
                return Err(format!("reorder references unknown workspace: {id}"));
            }
        }
        let mut reordered = Vec::with_capacity(ids.len());
        for id in ids {
            let idx = self
                .workspaces
                .iter()
                .position(|w| &w.id == id)
                .expect("validated above");
            reordered.push(self.workspaces.remove(idx));
        }
        self.workspaces = reordered;
        Ok(())
    }

    pub fn set_color(&mut self, id: &str, color: Option<String>) -> Result<(), String> {
        let ws = self
            .get_mut(id)
            .ok_or_else(|| format!("workspace not found: {id}"))?;
        ws.color = color;
        ws.updated_at = now_ms();
        Ok(())
    }

    /// 워크스페이스 시작 폴더를 설정하거나(Some) 해제한다(None/공백).
    /// 유효한 폴더로 설정하면 그 워크스페이스 트리의 **모든** `PaneLeaf.cwd`를
    /// 그 폴더로 덮어쓰고 재작성된 팬 개수를 반환한다. 해제 시에는 `root_dir`만
    /// `None`으로 되돌리고 기존 cwd는 보존한다 (R5/R7).
    ///
    /// VALIDATE-BEFORE-MUTATE (reorder와 동일한 계약): `normalize_root_dir`을
    /// `get_mut`보다 **먼저** 호출해, 잘못된 경로가 워크스페이스 상태에 절대
    /// 반쯤 적용되지 않게 한다.
    pub fn set_root_dir(&mut self, id: &str, root_dir: Option<String>) -> Result<usize, String> {
        let dir = normalize_root_dir(root_dir)?;
        let ws = self
            .get_mut(id)
            .ok_or_else(|| format!("workspace not found: {id}"))?;
        ws.root_dir = dir.clone();
        let mut n = 0;
        if let Some(d) = dir {
            for leaf in layout::collect_panes_mut(&mut ws.root) {
                leaf.cwd = d.clone();
                n += 1;
            }
        }
        ws.updated_at = now_ms();
        Ok(n)
    }

    /// Create a workspace from a prebuilt pane tree (template apply, Phase B).
    /// Enforces the same caps as `create`.
    pub fn create_with_root(
        &mut self,
        name: &str,
        root: PaneNode,
    ) -> Result<(Workspace, Option<String>), String> {
        if self.workspaces.len() >= WORKSPACE_HARD_CAP {
            return Err(format!(
                "workspace hard cap ({WORKSPACE_HARD_CAP}) reached; delete a workspace first"
            ));
        }
        let warning = if self.workspaces.len() >= WORKSPACE_SOFT_CAP {
            Some(format!(
                "workspace soft cap ({WORKSPACE_SOFT_CAP}) exceeded; consider closing unused workspaces"
            ))
        } else {
            None
        };
        let now = now_ms();
        let active = Some(layout::first_pane_id(&root));
        let ws = Workspace {
            id: new_id(),
            name: name.to_string(),
            root,
            active_pane_id: active,
            created_at: now,
            updated_at: now,
            color: None,
            // 템플릿에서 루트를 유도하지 않는다 (spec.md §E). None만 추가한다.
            root_dir: None,
        };
        self.workspaces.push(ws.clone());
        Ok((ws, warning))
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let ws = self
            .get_mut(id)
            .ok_or_else(|| format!("workspace not found: {id}"))?;
        ws.name = name.to_string();
        ws.updated_at = now_ms();
        Ok(())
    }

    /// Delete a workspace. If it was the last one, a fresh default workspace
    /// is created so the app always has at least one workspace.
    /// Returns the id of the workspace that should become active.
    pub fn delete(&mut self, id: &str) -> Result<WorkspaceId, String> {
        let idx = self
            .workspaces
            .iter()
            .position(|w| w.id == id)
            .ok_or_else(|| format!("workspace not found: {id}"))?;
        self.workspaces.remove(idx);
        if self.workspaces.is_empty() {
            self.workspaces.push(new_workspace("default"));
        }
        let next_active = match &self.active_workspace_id {
            Some(active) if active == id => {
                let fallback = self.workspaces[idx.min(self.workspaces.len() - 1)].id.clone();
                self.active_workspace_id = Some(fallback.clone());
                fallback
            }
            Some(active) => active.clone(),
            None => self.workspaces[0].id.clone(),
        };
        Ok(next_active)
    }
}

pub fn new_workspace(name: &str) -> Workspace {
    let now = now_ms();
    let leaf = layout::new_pane_leaf(&default_cwd());
    let pane_id = leaf.id.clone();
    Workspace {
        id: new_id(),
        name: name.to_string(),
        root: PaneNode::Pane(leaf),
        active_pane_id: Some(pane_id),
        created_at: now,
        updated_at: now,
        color: None,
        // #[serde(default)]는 역직렬화 전용이므로 struct 리터럴에는 명시 필요.
        root_dir: None,
    }
}

pub struct AppState {
    pub store: Mutex<WorkspaceStore>,
    pub registry: Arc<SessionRegistry>,
    pub config_path: PathBuf,
    /// Global injection kill switch (M2.0). When true, inject_prompt and
    /// rule-driven injection are refused; the user's own typing (write_pane)
    /// is never affected.
    pub injection_paused: std::sync::atomic::AtomicBool,
    /// Automation rule engine state (M2.1).
    pub automation: Mutex<crate::automation::AutomationState>,
}

/// Resolve an injection target across all workspaces (M2.0, roadmap §2.2):
/// either an explicit pane id or a label owned by exactly one pane.
/// Returns (workspace_id, pane_id, allow_injection).
pub fn resolve_inject_target(
    store: &WorkspaceStore,
    pane_id: Option<&str>,
    label: Option<&str>,
) -> Result<(WorkspaceId, String, bool), String> {
    match (pane_id, label) {
        (Some(pid), _) => {
            for ws in &store.workspaces {
                if let Some(leaf) = layout::find_pane(&ws.root, pid) {
                    return Ok((ws.id.clone(), leaf.id.clone(), leaf.allow_injection));
                }
            }
            Err(format!("pane not found: {pid}"))
        }
        (None, Some(label)) => {
            let mut matches: Vec<(WorkspaceId, String, bool)> = Vec::new();
            for ws in &store.workspaces {
                for leaf in layout::collect_panes(&ws.root) {
                    if leaf.labels.iter().any(|l| l == label) {
                        matches.push((ws.id.clone(), leaf.id.clone(), leaf.allow_injection));
                    }
                }
            }
            match matches.len() {
                0 => Err(format!("no pane has label '{label}'")),
                1 => Ok(matches.remove(0)),
                n => Err(format!(
                    "label '{label}' is ambiguous ({n} panes); use a unique label or an explicit paneId"
                )),
            }
        }
        (None, None) => Err("inject_prompt requires an explicit paneId or label".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_rename_delete_workspace() {
        let mut store = WorkspaceStore::with_default();
        let (ws, warning) = store.create("proj-a").unwrap();
        assert!(warning.is_none());
        assert_eq!(store.workspaces.len(), 2);
        store.rename(&ws.id, "proj-b").unwrap();
        assert_eq!(store.get(&ws.id).unwrap().name, "proj-b");
        store.delete(&ws.id).unwrap();
        assert_eq!(store.workspaces.len(), 1);
        assert!(store.get(&ws.id).is_none());
    }

    #[test]
    fn soft_cap_warns_hard_cap_refuses() {
        let mut store = WorkspaceStore::with_default();
        let mut warned = 0;
        for i in 1..WORKSPACE_HARD_CAP {
            let (_, warning) = store.create(&format!("ws{i}")).unwrap();
            if warning.is_some() {
                warned += 1;
            }
        }
        assert_eq!(store.workspaces.len(), WORKSPACE_HARD_CAP);
        assert_eq!(warned, WORKSPACE_HARD_CAP - WORKSPACE_SOFT_CAP);
        assert!(store.create("overflow").is_err());
    }

    #[test]
    fn delete_active_workspace_moves_active() {
        let mut store = WorkspaceStore::with_default();
        let first = store.workspaces[0].id.clone();
        let (second, _) = store.create("second").unwrap();
        store.active_workspace_id = Some(first.clone());
        let next = store.delete(&first).unwrap();
        assert_eq!(next, second.id);
        assert_eq!(store.active_workspace_id.as_ref(), Some(&second.id));
    }

    #[test]
    fn delete_last_workspace_recreates_default() {
        let mut store = WorkspaceStore::with_default();
        let only = store.workspaces[0].id.clone();
        let next = store.delete(&only).unwrap();
        assert_eq!(store.workspaces.len(), 1);
        assert_eq!(store.workspaces[0].name, "default");
        assert_eq!(next, store.workspaces[0].id);
    }

    #[test]
    fn delete_inactive_keeps_active() {
        let mut store = WorkspaceStore::with_default();
        let first = store.workspaces[0].id.clone();
        let (second, _) = store.create("second").unwrap();
        store.active_workspace_id = Some(first.clone());
        let next = store.delete(&second.id).unwrap();
        assert_eq!(next, first);
    }

    #[test]
    fn reorder_workspaces() {
        let mut store = WorkspaceStore::with_default();
        let a = store.workspaces[0].id.clone();
        let (b, _) = store.create("b").unwrap();
        let (c, _) = store.create("c").unwrap();
        store.reorder(&[c.id.clone(), a.clone(), b.id.clone()]).unwrap();
        let order: Vec<&str> = store.workspaces.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(order, vec![c.id.as_str(), a.as_str(), b.id.as_str()]);
        // invalid: wrong length
        assert!(store.reorder(&[a.clone()]).is_err());
        // invalid: unknown id (duplicate of one, missing another)
        assert!(store
            .reorder(&[a.clone(), a.clone(), "ghost".into()])
            .is_err());
        // failed reorders must not lose workspaces
        assert_eq!(store.workspaces.len(), 3);
    }

    #[test]
    fn set_color_roundtrip() {
        let mut store = WorkspaceStore::with_default();
        let id = store.workspaces[0].id.clone();
        store.set_color(&id, Some("#f38ba8".into())).unwrap();
        assert_eq!(store.metas()[0].color.as_deref(), Some("#f38ba8"));
        store.set_color(&id, None).unwrap();
        assert_eq!(store.metas()[0].color, None);
        assert!(store.set_color("ghost", None).is_err());
    }

    #[test]
    fn inject_target_resolution() {
        let mut store = WorkspaceStore::with_default();
        let ws1 = store.workspaces[0].id.clone();
        let pane1 = layout::first_pane_id(&store.workspaces[0].root);
        // label + allow on pane1
        {
            let ws = store.get_mut(&ws1).unwrap();
            let leaf = layout::collect_panes_mut(&mut ws.root).remove(0);
            leaf.labels = vec!["codex".into()];
            leaf.allow_injection = true;
        }
        // explicit pane id
        let (w, p, allow) = resolve_inject_target(&store, Some(&pane1), None).unwrap();
        assert_eq!((w.as_str(), p.as_str(), allow), (ws1.as_str(), pane1.as_str(), true));
        // unique label, found across workspaces
        let (_, p, _) = resolve_inject_target(&store, None, Some("codex")).unwrap();
        assert_eq!(p, pane1);
        // unknown label / pane
        assert!(resolve_inject_target(&store, None, Some("ghost")).is_err());
        assert!(resolve_inject_target(&store, Some("ghost"), None).is_err());
        assert!(resolve_inject_target(&store, None, None).is_err());
        // ambiguous label -> refused
        let (ws2, _) = store.create("two").unwrap();
        {
            let ws = store.get_mut(&ws2.id).unwrap();
            let leaf = layout::collect_panes_mut(&mut ws.root).remove(0);
            leaf.labels = vec!["codex".into()];
        }
        let err = resolve_inject_target(&store, None, Some("codex")).unwrap_err();
        assert!(err.contains("ambiguous"), "{err}");
    }

    #[test]
    fn from_config_repairs_stale_state() {
        let mut cfg = WorkspaceStore::with_default().to_config(Vec::new());
        cfg.active_workspace_id = Some("ghost".into());
        cfg.workspaces[0].active_pane_id = Some("ghost-pane".into());
        let store = WorkspaceStore::from_config(cfg);
        let ws0 = &store.workspaces[0];
        assert_eq!(store.active_workspace_id.as_ref(), Some(&ws0.id));
        assert_eq!(
            ws0.active_pane_id.as_deref(),
            Some(layout::first_pane_id(&ws0.root).as_str())
        );
    }

    // ---- set_root_dir (SPEC-WORKSPACE-ROOT-001 M1) ----

    /// `C:\Windows`는 autotest가 "guaranteed to exist"로 쓰는 보장된 폴더다.
    fn existing_dir() -> String {
        "C:\\Windows".to_string()
    }

    /// 팬 2개를 가진 store와 그 워크스페이스 id를 만든다.
    fn two_pane_store() -> (WorkspaceStore, WorkspaceId) {
        let mut store = WorkspaceStore::with_default();
        let id = store.workspaces[0].id.clone();
        let first = layout::first_pane_id(&store.workspaces[0].root);
        {
            let ws = store.get_mut(&id).unwrap();
            layout::split_pane(
                &mut ws.root,
                &first,
                crate::model::Direction::Row,
                layout::new_pane_leaf("C:\\seed"),
            )
            .unwrap();
        }
        (store, id)
    }

    #[test]
    fn set_root_dir_rewrites_leaf_cwds() {
        let (mut store, id) = two_pane_store();
        let root = existing_dir();
        let n = store.set_root_dir(&id, Some(root.clone())).unwrap();
        assert_eq!(n, 2, "both panes rewritten");
        for leaf in layout::collect_panes(&store.get(&id).unwrap().root) {
            assert_eq!(leaf.cwd, root, "pane cwd rewritten to root");
        }
        assert_eq!(
            store.get(&id).unwrap().root_dir.as_deref(),
            Some(root.as_str()),
            "workspace root_dir set"
        );
    }

    #[test]
    fn set_root_dir_rejects_missing_folder() {
        let (mut store, id) = two_pane_store();
        let before: Vec<String> = layout::collect_panes(&store.get(&id).unwrap().root)
            .iter()
            .map(|l| l.cwd.clone())
            .collect();
        let missing = "C:\\this\\path\\does\\not\\exist\\zzz12345";
        let res = store.set_root_dir(&id, Some(missing.into()));
        assert!(res.is_err(), "missing folder rejected");
        assert!(
            store.get(&id).unwrap().root_dir.is_none(),
            "root_dir unchanged on reject"
        );
        let after: Vec<String> = layout::collect_panes(&store.get(&id).unwrap().root)
            .iter()
            .map(|l| l.cwd.clone())
            .collect();
        assert_eq!(before, after, "cwds unchanged (validate-before-mutate)");
    }

    #[test]
    fn set_root_dir_rejects_file_path() {
        let (mut store, id) = two_pane_store();
        // 테스트 바이너리 자신은 실재하는 "파일"(디렉터리가 아님)이다.
        let file = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        let res = store.set_root_dir(&id, Some(file));
        assert!(res.is_err(), "file path (non-dir) rejected");
        assert!(store.get(&id).unwrap().root_dir.is_none());
    }

    #[test]
    fn set_root_dir_unknown_workspace_errors() {
        let (mut store, _id) = two_pane_store();
        // 존재하는 폴더 + 없는 워크스페이스 id → normalize 통과 후 get_mut 실패.
        let err = store.set_root_dir("ghost", Some(existing_dir())).unwrap_err();
        assert!(err.contains("workspace not found"), "{err}");
    }

    #[test]
    fn set_root_dir_blank_clears() {
        let (mut store, id) = two_pane_store();
        store.set_root_dir(&id, Some(existing_dir())).unwrap();
        assert!(store.get(&id).unwrap().root_dir.is_some());
        // 공백 문자열은 None으로 해제된다.
        let n = store.set_root_dir(&id, Some("   ".into())).unwrap();
        assert_eq!(n, 0, "clear rewrites no panes");
        assert!(
            store.get(&id).unwrap().root_dir.is_none(),
            "blank clears root_dir"
        );
    }

    #[test]
    fn set_root_dir_clear_keeps_existing_cwds() {
        let (mut store, id) = two_pane_store();
        let root = existing_dir();
        store.set_root_dir(&id, Some(root.clone())).unwrap();
        let cwds_after_set: Vec<String> = layout::collect_panes(&store.get(&id).unwrap().root)
            .iter()
            .map(|l| l.cwd.clone())
            .collect();
        assert!(cwds_after_set.iter().all(|c| *c == root));
        // None으로 해제: root_dir만 None, 팬 cwd는 보존 (R7).
        let n = store.set_root_dir(&id, None).unwrap();
        assert_eq!(n, 0, "clear rewrites no panes");
        assert!(store.get(&id).unwrap().root_dir.is_none());
        let cwds_after_clear: Vec<String> = layout::collect_panes(&store.get(&id).unwrap().root)
            .iter()
            .map(|l| l.cwd.clone())
            .collect();
        assert_eq!(
            cwds_after_set, cwds_after_clear,
            "cwds preserved on clear (R7)"
        );
    }
}
