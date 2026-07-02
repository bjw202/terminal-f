use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub type PaneId = String;
pub type WorkspaceId = String;
pub type SessionId = String;

/// v1: M0 initial schema. v2: adds optional `Workspace.color` (Phase A).
/// v3: adds `PaneLeaf.labels` and `PaneLeaf.allowInjection` (M2.0).
/// v4: adds `Config.automation` rule list (M2.1).
/// v5: adds `Rule.source` (tagged trigger; timer support, M2.1.5).
/// v6: adds `PaneLeaf.allowObserve` (control-API output subscription, M2.2).
/// v7: adds `PaneLeaf.startupCommand` + `Config.trustedRepos` (templates, Phase B).
pub const CONFIG_SCHEMA_VERSION: u32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Left/right split (CSS flex-direction: row)
    Row,
    /// Top/bottom split (CSS flex-direction: column)
    Column,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneLeaf {
    pub id: PaneId,
    /// Runtime-only. Never persisted; sanitized to None on config save.
    pub session_id: Option<SessionId>,
    pub cwd: String,
    /// Explicit command to run instead of the default shell. None = default shell.
    pub command: Option<String>,
    /// User-assigned labels ("codex", "reviewer", …). Automation targets
    /// panes by label, never by raw pane id (M2.0, roadmap §2.2).
    #[serde(default)]
    pub labels: Vec<String>,
    /// Per-pane opt-in for prompt injection. Default false: no automation
    /// or manual inject command may write to this pane (M2.0, roadmap §2.3).
    #[serde(default)]
    pub allow_injection: bool,
    /// Per-pane opt-in for output observation by the control API. Default
    /// false: terminal contents (which may hold secrets) are not exposed to
    /// external brokers unless enabled (M2.2, roadmap §2.7 #2).
    #[serde(default)]
    pub allow_observe: bool,
    /// Command typed into the shell once it is ready (Phase B templates).
    /// Unlike `command` (which replaces the shell), this runs inside the
    /// default shell, so the pane stays a live shell after it finishes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitNode {
    pub id: String,
    pub direction: Direction,
    /// flex ratio of `first` child; `second` gets 1 - ratio. Clamped to [0.1, 0.9].
    pub ratio: f32,
    pub first: Box<PaneNode>,
    pub second: Box<PaneNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PaneNode {
    Pane(PaneLeaf),
    Split(SplitNode),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub root: PaneNode,
    pub active_pane_id: Option<PaneId>,
    pub created_at: u64,
    pub updated_at: u64,
    /// Sidebar color label (CSS color string). Added in schema v2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub schema_version: u32,
    pub active_workspace_id: Option<WorkspaceId>,
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub ui: serde_json::Value,
    /// Automation rules (M2.1). Empty by default; older configs omit it.
    #[serde(default)]
    pub automation: Vec<crate::automation::Rule>,
    /// Repo paths the user has trusted to auto-run their `.terminal-f/profile.json`
    /// startup commands (Phase B workspace trust).
    #[serde(default)]
    pub trusted_repos: Vec<String>,
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn default_cwd() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_node_json_shape_matches_frontend_contract() {
        let node = PaneNode::Split(SplitNode {
            id: "s1".into(),
            direction: Direction::Row,
            ratio: 0.5,
            first: Box::new(PaneNode::Pane(PaneLeaf {
                id: "p1".into(),
                session_id: None,
                cwd: "C:/".into(),
                command: None,
                labels: Vec::new(),
                allow_injection: false,
                allow_observe: false,
                startup_command: None,
            })),
            second: Box::new(PaneNode::Pane(PaneLeaf {
                id: "p2".into(),
                session_id: Some("sess".into()),
                cwd: "C:/".into(),
                command: None,
                labels: vec!["codex".into()],
                allow_injection: true,
                allow_observe: true,
                startup_command: None,
            })),
        });
        let v = serde_json::to_value(&node).unwrap();
        assert_eq!(v["kind"], "split");
        assert_eq!(v["direction"], "row");
        assert_eq!(v["first"]["kind"], "pane");
        assert_eq!(v["second"]["sessionId"], "sess");
        assert_eq!(v["second"]["allowInjection"], true);
        assert_eq!(v["second"]["labels"][0], "codex");
        assert_eq!(v["first"]["allowInjection"], false);

        let back: PaneNode = serde_json::from_value(v).unwrap();
        match back {
            PaneNode::Split(s) => assert_eq!(s.id, "s1"),
            _ => panic!("expected split"),
        }
    }
}
