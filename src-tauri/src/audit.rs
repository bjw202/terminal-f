//! Append-only injection audit log (M2.0, roadmap §2.3 #4).
//!
//! Every prompt injection — manual or (later) rule-driven — is recorded as
//! one JSON line: when, triggered by what, into which pane/session, and a
//! bounded preview of the content. The file lives next to config.json.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub const PREVIEW_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// Unix millis.
    pub ts: u64,
    /// "manual" for palette-triggered injections; rule id once M2.1 lands.
    pub source: String,
    pub workspace_id: String,
    pub pane_id: String,
    pub session_id: String,
    pub bytes: usize,
    pub submitted: bool,
    pub bracketed: bool,
    /// First 200 chars of the injected text (control chars escaped).
    pub preview: String,
}

pub fn preview_of(text: &str) -> String {
    text.chars()
        .take(PREVIEW_MAX_CHARS)
        .map(|c| if c.is_control() && c != ' ' { '\u{2400}' } else { c })
        .collect()
}

pub fn append(path: &Path, entry: &AuditEntry) -> Result<(), String> {
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("audit log open failed: {e}"))?;
    writeln!(file, "{line}").map_err(|e| format!("audit log write failed: {e}"))
}

/// Read the last `limit` entries (newest last). Unparseable lines are
/// skipped rather than failing the whole read.
pub fn read_tail(path: &Path, limit: usize) -> Vec<AuditEntry> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(limit);
    lines[start..]
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::now_ms;

    fn entry(source: &str) -> AuditEntry {
        AuditEntry {
            ts: now_ms(),
            source: source.into(),
            workspace_id: "ws".into(),
            pane_id: "p".into(),
            session_id: "s".into(),
            bytes: 10,
            submitted: true,
            bracketed: false,
            preview: preview_of("echo hi\r\nnext"),
        }
    }

    #[test]
    fn append_and_read_tail() {
        let dir = std::env::temp_dir().join(format!("termf-audit-{}", crate::model::new_id()));
        let path = dir.join("audit.log");
        for i in 0..5 {
            append(&path, &entry(&format!("manual-{i}"))).unwrap();
        }
        let tail = read_tail(&path, 3);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail[0].source, "manual-2");
        assert_eq!(tail[2].source, "manual-4");
        assert_eq!(read_tail(&path, 100).len(), 5);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_missing_is_empty() {
        assert!(read_tail(Path::new("Z:/does/not/exist.log"), 10).is_empty());
    }

    #[test]
    fn preview_escapes_control_and_truncates() {
        let p = preview_of("a\x1b[200~b\r\nc");
        assert!(!p.contains('\x1b'));
        assert!(!p.contains('\r'));
        let long = "x".repeat(500);
        assert_eq!(preview_of(&long).chars().count(), PREVIEW_MAX_CHARS);
    }
}
