//! Project profile templates (Phase B, roadmap §3).
//!
//! A template is a pane-tree blueprint with runtime state stripped and
//! variables added. Applying it materializes a fresh workspace: variables are
//! substituted, new pane ids assigned, and per-pane `startupCommand`s run once
//! the shell is ready (see session spawn). Templates reuse the existing
//! `PaneNode` model and its invariants — no new tree rules.

use crate::layout::{self, DEFAULT_RATIO};
use crate::model::{new_id, Direction, PaneLeaf, PaneNode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MAX_TEMPLATE_PANES: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateParam {
    pub name: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
    /// UI hint only ("folder" | "text"); ignored by the backend.
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePane {
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub allow_injection: bool,
    #[serde(default)]
    pub allow_observe: bool,
    /// Replaces the shell (pane dies when it exits).
    #[serde(default)]
    pub command: Option<String>,
    /// Typed into the shell once ready (pane stays a shell).
    #[serde(default)]
    pub startup_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TemplateNode {
    Pane(TemplatePane),
    Split {
        direction: Direction,
        #[serde(default = "half")]
        ratio: f32,
        first: Box<TemplateNode>,
        second: Box<TemplateNode>,
    },
}

fn half() -> f32 {
    DEFAULT_RATIO
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub name: String,
    #[serde(default)]
    pub params: Vec<TemplateParam>,
    pub root: TemplateNode,
}

/// Whether the template runs any commands (used for the trust gate).
pub fn has_commands(node: &TemplateNode) -> bool {
    match node {
        TemplateNode::Pane(p) => p.command.is_some() || p.startup_command.is_some(),
        TemplateNode::Split { first, second, .. } => has_commands(first) || has_commands(second),
    }
}

fn count_panes(node: &TemplateNode) -> usize {
    match node {
        TemplateNode::Pane(_) => 1,
        TemplateNode::Split { first, second, .. } => count_panes(first) + count_panes(second),
    }
}

/// Substitute `${name}` (from params) and `${env:VAR}` (from process env).
/// Unknown placeholders are left intact.
pub fn substitute(input: &str, params: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = input[i + 2..].find('}') {
                let key = &input[i + 2..i + 2 + end];
                let replacement = if let Some(var) = key.strip_prefix("env:") {
                    std::env::var(var).ok()
                } else {
                    params.get(key).cloned()
                };
                if let Some(val) = replacement {
                    out.push_str(&val);
                    i += 2 + end + 1;
                    continue;
                }
                // unknown: leave the literal placeholder
                out.push_str(&input[i..i + 2 + end + 1]);
                i += 2 + end + 1;
                continue;
            }
        }
        out.push(input[i..].chars().next().unwrap());
        i += input[i..].chars().next().unwrap().len_utf8();
    }
    out
}

/// Validate a template before applying: at least one pane, within the pane
/// cap, and every declared param has a value (via `params` or its default).
pub fn validate(template: &Template, params: &HashMap<String, String>) -> Result<(), String> {
    let panes = count_panes(&template.root);
    if panes == 0 {
        return Err("template has no panes".into());
    }
    if panes > MAX_TEMPLATE_PANES {
        return Err(format!(
            "template has {panes} panes; max per workspace is {MAX_TEMPLATE_PANES}"
        ));
    }
    for p in &template.params {
        if !params.contains_key(&p.name) && p.default.is_none() {
            return Err(format!("missing value for template parameter '{}'", p.name));
        }
    }
    Ok(())
}

/// Build a concrete pane tree from a template, substituting variables and
/// assigning fresh ids. Param defaults fill any unspecified params.
pub fn build_tree(template: &Template, params: &HashMap<String, String>) -> PaneNode {
    let mut resolved = params.clone();
    for p in &template.params {
        if !resolved.contains_key(&p.name) {
            if let Some(def) = &p.default {
                resolved.insert(p.name.clone(), def.clone());
            }
        }
    }
    build_node(&template.root, &resolved)
}

fn build_node(node: &TemplateNode, params: &HashMap<String, String>) -> PaneNode {
    match node {
        TemplateNode::Pane(p) => {
            let cwd = p
                .cwd
                .as_ref()
                .map(|c| substitute(c, params))
                .filter(|c| !c.trim().is_empty())
                .unwrap_or_else(crate::model::default_cwd);
            PaneNode::Pane(PaneLeaf {
                id: new_id(),
                session_id: None,
                cwd,
                command: p.command.as_ref().map(|c| substitute(c, params)),
                labels: p
                    .labels
                    .iter()
                    .map(|l| l.trim().to_lowercase())
                    .filter(|l| !l.is_empty())
                    .collect(),
                allow_injection: p.allow_injection,
                allow_observe: p.allow_observe,
                startup_command: p
                    .startup_command
                    .as_ref()
                    .map(|c| substitute(c, params))
                    .filter(|c| !c.trim().is_empty()),
            })
        }
        TemplateNode::Split {
            direction,
            ratio,
            first,
            second,
        } => PaneNode::Split(crate::model::SplitNode {
            id: new_id(),
            direction: *direction,
            ratio: layout::clamp_ratio(*ratio),
            first: Box::new(build_node(first, params)),
            second: Box::new(build_node(second, params)),
        }),
    }
}

/// Convert an existing pane tree back into a template blueprint (Save current
/// layout as template). Ids and session state are dropped.
pub fn from_pane_tree(node: &PaneNode) -> TemplateNode {
    match node {
        PaneNode::Pane(l) => TemplateNode::Pane(TemplatePane {
            cwd: Some(l.cwd.clone()),
            labels: l.labels.clone(),
            allow_injection: l.allow_injection,
            allow_observe: l.allow_observe,
            command: l.command.clone(),
            startup_command: l.startup_command.clone(),
        }),
        PaneNode::Split(s) => TemplateNode::Split {
            direction: s.direction,
            ratio: s.ratio,
            first: Box::new(from_pane_tree(&s.first)),
            second: Box::new(from_pane_tree(&s.second)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn substitute_params_and_env() {
        std::env::set_var("TERMF_TEST_VAR", "envval");
        let p = params(&[("repo", "C:/work/x")]);
        assert_eq!(substitute("${repo}/frontend", &p), "C:/work/x/frontend");
        assert_eq!(substitute("${env:TERMF_TEST_VAR}", &p), "envval");
        // unknown placeholder left intact
        assert_eq!(substitute("${nope}", &p), "${nope}");
        // no placeholders
        assert_eq!(substitute("plain", &p), "plain");
    }

    fn sample() -> Template {
        Template {
            name: "ai-pair".into(),
            params: vec![TemplateParam {
                name: "repo".into(),
                prompt: Some("repo?".into()),
                default: None,
                kind: Some("folder".into()),
            }],
            root: TemplateNode::Split {
                direction: Direction::Row,
                ratio: 0.5,
                first: Box::new(TemplateNode::Pane(TemplatePane {
                    cwd: Some("${repo}".into()),
                    labels: vec!["Claude".into()],
                    allow_injection: false,
                    allow_observe: false,
                    command: None,
                    startup_command: Some("claude".into()),
                })),
                second: Box::new(TemplateNode::Pane(TemplatePane {
                    cwd: Some("${repo}".into()),
                    labels: vec!["codex".into()],
                    allow_injection: true,
                    allow_observe: true,
                    command: None,
                    startup_command: Some("echo ready in ${repo}".into()),
                })),
            },
        }
    }

    #[test]
    fn validate_requires_params() {
        let t = sample();
        assert!(validate(&t, &params(&[])).is_err());
        assert!(validate(&t, &params(&[("repo", "C:/x")])).is_ok());
    }

    #[test]
    fn build_tree_substitutes_and_assigns_ids() {
        let t = sample();
        let tree = build_tree(&t, &params(&[("repo", "C:/work/x")]));
        layout::check_invariants(&tree).unwrap();
        let panes = layout::collect_panes(&tree);
        assert_eq!(panes.len(), 2);
        assert!(panes.iter().all(|p| !p.id.is_empty()));
        // labels lowercased, startup substituted
        let codex = panes.iter().find(|p| p.labels.contains(&"codex".to_string())).unwrap();
        assert!(codex.allow_injection);
        assert_eq!(codex.startup_command.as_deref(), Some("echo ready in C:/work/x"));
        let claude = panes.iter().find(|p| p.labels.contains(&"claude".to_string())).unwrap();
        assert_eq!(claude.cwd, "C:/work/x");
        assert_eq!(claude.startup_command.as_deref(), Some("claude"));
    }

    #[test]
    fn has_commands_detects_startup() {
        assert!(has_commands(&sample().root));
        let plain = TemplateNode::Pane(TemplatePane {
            cwd: None,
            labels: vec![],
            allow_injection: false,
            allow_observe: false,
            command: None,
            startup_command: None,
        });
        assert!(!has_commands(&plain));
    }

    #[test]
    fn pane_cap_enforced() {
        // build a deep template of 17 panes
        fn deep(n: usize) -> TemplateNode {
            if n <= 1 {
                TemplateNode::Pane(TemplatePane {
                    cwd: None,
                    labels: vec![],
                    allow_injection: false,
                    allow_observe: false,
                    command: None,
                    startup_command: None,
                })
            } else {
                TemplateNode::Split {
                    direction: Direction::Row,
                    ratio: 0.5,
                    first: Box::new(deep(1)),
                    second: Box::new(deep(n - 1)),
                }
            }
        }
        let t = Template {
            name: "big".into(),
            params: vec![],
            root: deep(17),
        };
        assert!(validate(&t, &params(&[])).is_err());
    }

    #[test]
    fn roundtrip_from_pane_tree() {
        let t = sample();
        let tree = build_tree(&t, &params(&[("repo", "C:/x")]));
        let back = from_pane_tree(&tree);
        // rebuilding from the blueprint yields the same pane count
        let t2 = Template {
            name: "rt".into(),
            params: vec![],
            root: back,
        };
        let tree2 = build_tree(&t2, &params(&[]));
        assert_eq!(layout::collect_panes(&tree2).len(), 2);
    }
}
