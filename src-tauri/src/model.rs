use serde::{Deserialize, Serialize};
use std::path::Path;
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
/// v8: adds optional `Workspace.root_dir` (워크스페이스 시작 폴더,
/// SPEC-WORKSPACE-ROOT-001).
pub const CONFIG_SCHEMA_VERSION: u32 = 8;

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
    /// 워크스페이스 시작 폴더(start folder). 설정되면 이 워크스페이스의 모든 팬이
    /// 이 폴더에서 셸을 연다. `root`(팬 트리)와는 완전히 다른 개념이므로 혼동하지
    /// 말 것. JSON 키는 "rootDir"(camelCase). 스키마 v8에서 추가.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_dir: Option<String>,
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

/// 순수 문자열 정규화 (파일시스템 무관, SPEC-WORKSPACE-ROOT-001 §A.1).
/// 규칙:
///   1. `trim()` 후 빈 문자열이면 `None`
///   2. 후행 `\` 또는 `/`를 **한 개만** 제거. 단 남은 길이가 3자 미만(`C:\`)이거나
///      경로가 `\\`로 시작(UNC 공유 루트)하면 제거하지 않는다
///   3. 파일시스템을 건드리지 않는다 → 어떤 경로 리터럴이든 결정적으로 테스트 가능
pub fn normalize_root_dir_str(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // UNC 공유 루트("\\" 접두)는 후행 구분자를 제거하지 않는다.
    let is_unc = trimmed.starts_with("\\\\");
    let last = trimmed.chars().last();
    if !is_unc && (last == Some('\\') || last == Some('/')) {
        // 후행 구분자는 ASCII 1바이트이므로 마지막 바이트를 잘라내면 안전하다.
        let without = &trimmed[..trimmed.len() - 1];
        // 남은 경로가 3자 이상일 때만 제거 (`C:\` → `C:` 방지).
        if without.chars().count() >= 3 {
            return Some(without.to_string());
        }
    }
    Some(trimmed.to_string())
}

/// 존재 검증 계층 (SPEC-WORKSPACE-ROOT-001 §A.1).
///   1. `raw`를 `normalize_root_dir_str`에 위임. `None`이면 `Ok(None)`
///   2. `Some(p)`이면 `Path::new(&p).is_dir()`가 참이어야 하며, 거짓이면 `Err`
///   3. `fs::canonicalize`를 호출하지 않는다 — Windows `\\?\` 확장 길이 접두사가
///      UI 표시와 `CommandBuilder::cwd`로 새어 나간다
pub fn normalize_root_dir(raw: Option<String>) -> Result<Option<String>, String> {
    let normalized = match raw {
        Some(s) => normalize_root_dir_str(&s),
        None => None,
    };
    match normalized {
        None => Ok(None),
        Some(p) => {
            if Path::new(&p).is_dir() {
                Ok(Some(p))
            } else {
                Err(format!("not an existing folder: {p}"))
            }
        }
    }
}

/// 이음새(seam, SPEC-WORKSPACE-ROOT-001 §A.5): 루트를 아는 호출자가 cwd와
/// root_dir을 직접 공급하는 형태. 오늘 호출 지점은 없다 — store 참조를 자유
/// 함수까지 관통시키는 설계를 훗날 막기 위한 것이다.
pub fn new_workspace_in(name: &str, cwd: &str, root_dir: Option<String>) -> Workspace {
    let now = now_ms();
    let leaf = crate::layout::new_pane_leaf(cwd);
    let pane_id = leaf.id.clone();
    Workspace {
        id: new_id(),
        name: name.to_string(),
        root: PaneNode::Pane(leaf),
        active_pane_id: Some(pane_id),
        created_at: now,
        updated_at: now,
        color: None,
        root_dir,
    }
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

    // ---- normalize_root_dir_str: 순수 문자열 규칙 (AC-10a, 파일시스템 무관) ----

    #[test]
    fn normalize_str_strips_one_trailing_separator() {
        assert_eq!(
            normalize_root_dir_str("C:\\work\\"),
            Some("C:\\work".to_string())
        );
        assert_eq!(
            normalize_root_dir_str("C:\\work/"),
            Some("C:\\work".to_string()),
            "forward slash also stripped"
        );
    }

    #[test]
    fn normalize_str_no_trailing_is_unchanged() {
        assert_eq!(
            normalize_root_dir_str("C:\\work"),
            Some("C:\\work".to_string())
        );
    }

    #[test]
    fn normalize_str_keeps_drive_root() {
        // "C:\" → "C:" 는 3자 미만이 되므로 구분자를 제거하지 않는다.
        assert_eq!(normalize_root_dir_str("C:\\"), Some("C:\\".to_string()));
    }

    #[test]
    fn normalize_str_keeps_unc_prefix() {
        // UNC 가드: "\\"로 시작하면 후행 구분자를 제거하지 않는다.
        // 이 케이스는 가드를 삭제하면 "\\host\share"가 되어 실패한다 (비공허).
        assert_eq!(
            normalize_root_dir_str("\\\\host\\share\\"),
            Some("\\\\host\\share\\".to_string())
        );
    }

    #[test]
    fn normalize_str_unc_no_trailing_unchanged() {
        assert_eq!(
            normalize_root_dir_str("\\\\host\\share"),
            Some("\\\\host\\share".to_string())
        );
    }

    #[test]
    fn normalize_str_blank_is_none() {
        assert_eq!(normalize_root_dir_str("   "), None, "whitespace trims to None");
        assert_eq!(normalize_root_dir_str(""), None, "empty is None");
    }

    // ---- normalize_root_dir: 존재 검증 계층 (AC-10b, 파일시스템 의존) ----

    #[test]
    fn normalize_validated_accepts_existing_dir() {
        // C:\Windows 는 실재가 보장된 폴더. 후행 구분자가 제거된다.
        let r = normalize_root_dir(Some("C:\\Windows\\".to_string())).unwrap();
        assert_eq!(r, Some("C:\\Windows".to_string()));
    }

    #[test]
    fn normalize_validated_accepts_temp_dir_strips_sep() {
        let sub = std::env::temp_dir().join(format!("termf-nrd-{}", new_id()));
        std::fs::create_dir_all(&sub).unwrap();
        let expected = sub.to_string_lossy().into_owned();
        let with_sep = format!("{expected}\\");
        let r = normalize_root_dir(Some(with_sep)).unwrap();
        assert_eq!(r, Some(expected), "existing temp dir, trailing sep stripped");
        std::fs::remove_dir_all(&sub).ok();
    }

    #[test]
    fn normalize_validated_rejects_missing() {
        let r = normalize_root_dir(Some("C:\\nope\\zzz\\missing12345".to_string()));
        assert!(r.is_err(), "non-existent path rejected");
    }

    #[test]
    fn normalize_validated_rejects_file_path() {
        // 테스트 바이너리 자신은 실재하는 파일(디렉터리 아님)이다.
        let file = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        let r = normalize_root_dir(Some(file));
        assert!(r.is_err(), "a file path is not a directory");
    }

    #[test]
    fn normalize_validated_none_is_ok_none() {
        assert_eq!(normalize_root_dir(None).unwrap(), None);
        assert_eq!(
            normalize_root_dir(Some("   ".to_string())).unwrap(),
            None,
            "blank never reaches is_dir()"
        );
    }
}
