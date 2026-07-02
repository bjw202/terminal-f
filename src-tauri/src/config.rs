//! config.json persistence.
//!
//! Persisted: schemaVersion, workspaces (layout tree, pane cwd, pane command),
//! activeWorkspaceId, optional UI prefs.
//! NOT persisted: session ids, process state, scrollback, raw output history.
//! On restart the layout is restored and fresh shells are spawned lazily.

use crate::layout;
use crate::model::{Config, PaneNode, CONFIG_SCHEMA_VERSION};
use std::fs;
use std::path::Path;

/// Migration entry point. Supported paths:
///   v1 (M0) -> v2: `Workspace.color` added as an optional field.
///   v2 (Phase A) -> v3: `PaneLeaf.labels` + `PaneLeaf.allowInjection` added
///   with serde defaults.
/// All added fields default cleanly, so migrating any older version is
/// parsing with defaults and re-stamping the version. See the fixture tests.
pub fn migrate(value: serde_json::Value) -> Result<Config, String> {
    let version = value
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    match version {
        CONFIG_SCHEMA_VERSION => {
            serde_json::from_value(value).map_err(|e| format!("config parse error: {e}"))
        }
        1 | 2 | 3 | 4 | 5 | 6 => {
            let mut cfg: Config = serde_json::from_value(value)
                .map_err(|e| format!("config v{version} parse error: {e}"))?;
            cfg.schema_version = CONFIG_SCHEMA_VERSION;
            Ok(cfg)
        }
        other => Err(format!(
            "unsupported config schemaVersion {other} (supported: 1..={CONFIG_SCHEMA_VERSION})"
        )),
    }
}

/// Load config from disk. Ok(None) when the file does not exist yet.
pub fn load_config(path: &Path) -> Result<Option<Config>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("config read error: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("config is not valid JSON: {e}"))?;
    let mut cfg = migrate(value)?;
    sanitize(&mut cfg);
    for ws in &cfg.workspaces {
        layout::check_invariants(&ws.root)
            .map_err(|e| format!("config workspace '{}' violates layout invariants: {e}", ws.name))?;
    }
    Ok(Some(cfg))
}

/// Atomic-ish save: write to a temp file then rename over the target.
pub fn save_config(path: &Path, cfg: &Config) -> Result<(), String> {
    let mut clean = cfg.clone();
    sanitize(&mut clean);
    let json = serde_json::to_string_pretty(&clean)
        .map_err(|e| format!("config serialize error: {e}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("config dir create error: {e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("config write error: {e}"))?;
    fs::rename(&tmp, path).or_else(|_| {
        // rename over an existing file can fail on some Windows setups; fall back
        fs::copy(&tmp, path).map(|_| ()).and_then(|_| fs::remove_file(&tmp))
    })
    .map_err(|e| format!("config rename error: {e}"))?;
    Ok(())
}

/// Strip runtime-only state before persisting / after loading.
fn sanitize(cfg: &mut Config) {
    cfg.schema_version = CONFIG_SCHEMA_VERSION;
    for ws in &mut cfg.workspaces {
        sanitize_node(&mut ws.root);
    }
}

fn sanitize_node(node: &mut PaneNode) {
    match node {
        PaneNode::Pane(l) => l.session_id = None,
        PaneNode::Split(s) => {
            sanitize_node(&mut s.first);
            sanitize_node(&mut s.second);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{new_pane_leaf, split_pane};
    use crate::model::{now_ms, Direction, Workspace};

    fn sample_config() -> Config {
        let mut root = PaneNode::Pane(new_pane_leaf("C:/"));
        let first = match &root {
            PaneNode::Pane(l) => l.id.clone(),
            _ => unreachable!(),
        };
        split_pane(&mut root, &first, Direction::Row, new_pane_leaf("C:/tmp")).unwrap();
        // simulate runtime session attachment; must be stripped on save
        if let PaneNode::Split(s) = &mut root {
            if let PaneNode::Pane(l) = &mut *s.first {
                l.session_id = Some("runtime-session".into());
            }
        }
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            active_workspace_id: Some("ws1".into()),
            workspaces: vec![Workspace {
                id: "ws1".into(),
                name: "default".into(),
                root,
                active_pane_id: Some(first),
                created_at: now_ms(),
                updated_at: now_ms(),
                color: None,
            }],
            ui: serde_json::json!({}),
            automation: Vec::new(),
            trusted_repos: Vec::new(),
        }
    }

    #[test]
    fn save_load_roundtrip_strips_session_ids() {
        let dir = std::env::temp_dir().join(format!("termf-test-{}", crate::model::new_id()));
        let path = dir.join("config.json");
        let cfg = sample_config();
        save_config(&path, &cfg).unwrap();
        let loaded = load_config(&path).unwrap().expect("config must exist");
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.active_workspace_id.as_deref(), Some("ws1"));
        for leaf in crate::layout::collect_panes(&loaded.workspaces[0].root) {
            assert!(leaf.session_id.is_none(), "session ids must never persist");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_returns_none() {
        let path = std::env::temp_dir().join("termf-does-not-exist").join("config.json");
        assert!(load_config(&path).unwrap().is_none());
    }

    #[test]
    fn v1_fixture_migrates_to_v2() {
        // Real v1 config shape as written by the M0 build (no `color` field).
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "activeWorkspaceId": "ws-legacy",
            "workspaces": [{
                "id": "ws-legacy",
                "name": "default",
                "root": {
                    "kind": "split",
                    "id": "split-1",
                    "direction": "row",
                    "ratio": 0.5,
                    "first": { "kind": "pane", "id": "p1", "sessionId": null,
                               "cwd": "C:\\Users\\someone", "command": null },
                    "second": { "kind": "pane", "id": "p2", "sessionId": null,
                                "cwd": "C:\\Users\\someone", "command": null }
                },
                "activePaneId": "p2",
                "createdAt": 1782956098600u64,
                "updatedAt": 1782956101324u64
            }],
            "ui": {}
        });
        let cfg = migrate(legacy).expect("v1 must migrate");
        assert_eq!(cfg.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(cfg.workspaces.len(), 1);
        assert_eq!(cfg.workspaces[0].color, None, "v2 color defaults to None");
        assert_eq!(cfg.active_workspace_id.as_deref(), Some("ws-legacy"));
        for leaf in crate::layout::collect_panes(&cfg.workspaces[0].root) {
            assert!(leaf.labels.is_empty(), "v3 labels default to empty");
            assert!(!leaf.allow_injection, "v3 allowInjection defaults to false");
        }
    }

    #[test]
    fn v2_fixture_migrates_to_v3() {
        // v2 shape as written by the Phase A build (color, no labels).
        let legacy = serde_json::json!({
            "schemaVersion": 2,
            "activeWorkspaceId": "ws-a",
            "workspaces": [{
                "id": "ws-a",
                "name": "default",
                "color": "#89b4fa",
                "root": { "kind": "pane", "id": "p1", "sessionId": null,
                          "cwd": "C:\\", "command": null },
                "activePaneId": "p1",
                "createdAt": 1u64,
                "updatedAt": 2u64
            }],
            "ui": { "theme": "one-dark" }
        });
        let cfg = migrate(legacy).expect("v2 must migrate");
        assert_eq!(cfg.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(cfg.workspaces[0].color.as_deref(), Some("#89b4fa"));
        let leaf = crate::layout::collect_panes(&cfg.workspaces[0].root)[0];
        assert!(leaf.labels.is_empty());
        assert!(!leaf.allow_injection);
        assert!(cfg.automation.is_empty(), "v4 automation defaults to empty");
    }

    #[test]
    fn v3_fixture_migrates_to_v4() {
        // v3 shape (labels/allowInjection present, no automation field).
        let legacy = serde_json::json!({
            "schemaVersion": 3,
            "activeWorkspaceId": "ws-a",
            "workspaces": [{
                "id": "ws-a", "name": "default", "color": null,
                "root": { "kind": "pane", "id": "p1", "sessionId": null,
                          "cwd": "C:\\", "command": null,
                          "labels": ["codex"], "allowInjection": true },
                "activePaneId": "p1", "createdAt": 1u64, "updatedAt": 2u64
            }],
            "ui": {}
        });
        let cfg = migrate(legacy).expect("v3 must migrate");
        assert_eq!(cfg.schema_version, CONFIG_SCHEMA_VERSION);
        let leaf = crate::layout::collect_panes(&cfg.workspaces[0].root)[0];
        assert_eq!(leaf.labels, vec!["codex".to_string()]);
        assert!(leaf.allow_injection);
        assert!(!leaf.allow_observe, "v6 allowObserve defaults to false");
        assert!(cfg.automation.is_empty());
    }

    #[test]
    fn v4_fixture_migrates_to_v5_with_legacy_rule() {
        // v4 rule shape: `repo` present, no `source` field.
        let legacy = serde_json::json!({
            "schemaVersion": 4,
            "activeWorkspaceId": "ws-a",
            "workspaces": [{
                "id": "ws-a", "name": "default", "color": null,
                "root": { "kind": "pane", "id": "p1", "sessionId": null,
                          "cwd": "C:\\", "command": null,
                          "labels": [], "allowInjection": false },
                "activePaneId": "p1", "createdAt": 1u64, "updatedAt": 2u64
            }],
            "ui": {},
            "automation": [{
                "id": "r1", "name": "legacy git rule", "enabled": true,
                "repo": "C:\\repo", "cooldownMs": 5000, "maxPerMin": 4,
                "targetLabel": "codex", "targetPane": null,
                "template": "review {{diffStat}}", "submit": true,
                "requireIdle": true, "mode": "confirm"
            }]
        });
        let cfg = migrate(legacy).expect("v4 must migrate");
        assert_eq!(cfg.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(cfg.automation.len(), 1);
        // legacy rule with no `source` resolves to a git-diff source on `repo`.
        assert_eq!(
            cfg.automation[0].effective_source(),
            crate::automation::RuleSource::GitDiff {
                repo: "C:\\repo".into()
            }
        );
    }

    #[test]
    fn migrate_rejects_unknown_schema_version() {
        let err = migrate(serde_json::json!({ "schemaVersion": 999 })).unwrap_err();
        assert!(err.contains("unsupported"));
        let err0 = migrate(serde_json::json!({ "workspaces": [] })).unwrap_err();
        assert!(err0.contains("unsupported"));
    }
}
