//! Broker rule engine (M2.1, roadmap §2.1/§2.4a).
//!
//! A rule watches a git repo; when its working-tree diff changes and settles,
//! the engine fires an action that injects a prompt into a labeled pane —
//! reusing the M2.0 injection path and its gates (ADR-006). Firing is
//! confirm-by-default: the backend emits a proposal and the user approves it
//! before anything is typed.
//!
//! This module holds the declarative rule model, the per-rule runtime state,
//! and the *pure* firing decision (`RuleRuntime::decide`) so cooldown /
//! rate-limit / dedup / debounce logic is unit-tested without git or a clock.
//! The git IO and the poll thread live in the command/lib layers.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Poll cadence of the automation thread.
pub const POLL_INTERVAL_MS: u64 = 2000;
/// Default per-rule cooldown between fires.
pub const DEFAULT_COOLDOWN_MS: u64 = 5000;
/// Default per-rule cap on fires per rolling 60 s (loop/rate guard).
pub const DEFAULT_MAX_PER_MIN: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleMode {
    /// Emit a proposal; inject only after explicit user approval (default).
    Confirm,
    /// Inject automatically (still passes allowlist + idle gates).
    Auto,
}

/// What a rule watches. Tagged enum so new sources (pane-output, …) can be
/// added without breaking the schema. Optional on `Rule` for back-compat:
/// pre-M2.1.5 rules have only `repo` and fall back to `GitDiff { repo }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuleSource {
    /// Fire when the repo's working tree changes (M2.1).
    GitDiff { repo: String },
    /// Fire on a fixed interval (M2.1.5). No git involved.
    /// (`rename_all` on the container renames variants, not their fields, so
    /// the camelCase wire name is set explicitly here.)
    Timer {
        #[serde(rename = "everyMs")]
        every_ms: u64,
    },
}

/// Minimum timer interval to avoid accidental hot loops.
pub const TIMER_MIN_MS: u64 = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Legacy git repo path (pre-M2.1.5). Kept for back-compat; new rules use
    /// `source`. See `effective_source`.
    #[serde(default)]
    pub repo: String,
    /// Trigger source. Absent in pre-M2.1.5 configs (falls back to `repo`).
    #[serde(default)]
    pub source: Option<RuleSource>,
    #[serde(default = "default_cooldown")]
    pub cooldown_ms: u64,
    #[serde(default = "default_max_per_min")]
    pub max_per_min: u32,
    /// Injection target: a pane label (preferred) or explicit pane id.
    #[serde(default)]
    pub target_label: Option<String>,
    #[serde(default)]
    pub target_pane: Option<String>,
    /// Prompt template. Tokens: {{summary}}, {{diffStat}}, {{files}}.
    pub template: String,
    #[serde(default = "default_true")]
    pub submit: bool,
    #[serde(default = "default_true")]
    pub require_idle: bool,
    #[serde(default = "default_mode")]
    pub mode: RuleMode,
}

fn default_true() -> bool {
    true
}
fn default_cooldown() -> u64 {
    DEFAULT_COOLDOWN_MS
}
fn default_max_per_min() -> u32 {
    DEFAULT_MAX_PER_MIN
}
fn default_mode() -> RuleMode {
    RuleMode::Confirm
}

impl Rule {
    /// Resolve the trigger source, falling back to the legacy `repo` field
    /// for rules persisted before `source` existed.
    pub fn effective_source(&self) -> RuleSource {
        self.source
            .clone()
            .unwrap_or_else(|| RuleSource::GitDiff {
                repo: self.repo.clone(),
            })
    }

    pub fn validate(&self) -> Result<(), String> {
        match self.effective_source() {
            RuleSource::GitDiff { repo } if repo.trim().is_empty() => {
                return Err("git-diff rule needs a repo path".into());
            }
            RuleSource::Timer { every_ms } if every_ms < TIMER_MIN_MS => {
                return Err(format!("timer interval must be >= {TIMER_MIN_MS}ms"));
            }
            _ => {}
        }
        if self.template.trim().is_empty() {
            return Err("rule template is empty".into());
        }
        if self.target_label.is_none() && self.target_pane.is_none() {
            return Err("rule needs a targetLabel or targetPane".into());
        }
        Ok(())
    }
}

/// Summary of a repo's working-tree changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitSummary {
    pub stat: String,
    pub files: Vec<String>,
    /// Content hash of the porcelain status; changes iff the working tree does.
    pub hash: String,
    pub changed: bool,
}

/// A rendered, ready-to-inject firing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub target_label: Option<String>,
    pub target_pane: Option<String>,
    pub text: String,
    pub submit: bool,
    pub require_idle: bool,
    pub summary: String,
}

#[derive(Debug)]
pub enum Decision {
    Fire,
    Skip(String),
}

/// Non-persisted per-rule runtime: debounce/cooldown/rate-limit/dedup state.
#[derive(Default)]
pub struct RuleRuntime {
    /// Hash of the diff we last fired on (dedup: don't refire same diff).
    last_fired_hash: Option<String>,
    last_fire_at: Option<Instant>,
    /// Fire instants within the rolling rate-limit window.
    fire_times: VecDeque<Instant>,
    /// Debounce: current diff hash must be seen twice in a row before firing.
    last_seen_hash: Option<String>,
    stable: u32,
    /// Timer rules: anchor for the first interval before any fire happens.
    timer_anchor: Option<Instant>,
}

impl RuleRuntime {
    /// Decide whether to fire this poll. Mutates debounce bookkeeping only;
    /// the caller calls `record_fire` when a fire actually happens.
    /// `manual=true` (run-now) bypasses debounce/cooldown/rate-limit/dedup but
    /// still requires that there is something to act on when git-derived.
    pub fn decide(
        &mut self,
        rule: &Rule,
        current_hash: Option<&str>,
        now: Instant,
        manual: bool,
    ) -> Decision {
        if manual {
            return Decision::Fire;
        }
        let Some(hash) = current_hash.filter(|h| !h.is_empty()) else {
            self.last_seen_hash = None;
            self.stable = 0;
            return Decision::Skip("no changes".into());
        };
        // Debounce: require the same hash across two consecutive polls so we
        // don't fire in the middle of a burst of edits.
        if self.last_seen_hash.as_deref() == Some(hash) {
            self.stable = self.stable.saturating_add(1);
        } else {
            self.last_seen_hash = Some(hash.to_string());
            self.stable = 0;
            return Decision::Skip("waiting for diff to settle".into());
        }
        if self.stable < 1 {
            return Decision::Skip("waiting for diff to settle".into());
        }
        // Dedup: same diff we already fired on.
        if self.last_fired_hash.as_deref() == Some(hash) {
            return Decision::Skip("already fired for this diff".into());
        }
        // Cooldown.
        if let Some(last) = self.last_fire_at {
            if now.duration_since(last) < Duration::from_millis(rule.cooldown_ms) {
                return Decision::Skip("cooldown".into());
            }
        }
        if self.rate_limited(rule, now) {
            return Decision::Skip("rate limited".into());
        }
        Decision::Fire
    }

    /// Firing decision for a timer source: fire once every `every_ms`,
    /// starting one interval after the rule is first observed.
    pub fn decide_timer(&mut self, rule: &Rule, every_ms: u64, now: Instant, manual: bool) -> Decision {
        if manual {
            return Decision::Fire;
        }
        let anchor = self.last_fire_at.or(self.timer_anchor);
        match anchor {
            None => {
                self.timer_anchor = Some(now);
                Decision::Skip("timer armed".into())
            }
            Some(t) => {
                if now.duration_since(t) < Duration::from_millis(every_ms) {
                    return Decision::Skip("waiting for interval".into());
                }
                if self.rate_limited(rule, now) {
                    return Decision::Skip("rate limited".into());
                }
                Decision::Fire
            }
        }
    }

    /// Prune the rolling-60 s fire window and report whether the rule is at
    /// its per-minute cap.
    fn rate_limited(&mut self, rule: &Rule, now: Instant) -> bool {
        let window = Duration::from_secs(60);
        while let Some(front) = self.fire_times.front() {
            if now.duration_since(*front) > window {
                self.fire_times.pop_front();
            } else {
                break;
            }
        }
        self.fire_times.len() as u32 >= rule.max_per_min
    }

    pub fn record_fire(&mut self, hash: Option<&str>, now: Instant) {
        if let Some(h) = hash {
            self.last_fired_hash = Some(h.to_string());
        }
        self.last_fire_at = Some(now);
        self.fire_times.push_back(now);
    }
}

/// Render a rule template against a git summary.
pub fn render_template(template: &str, summary: &GitSummary) -> String {
    template
        .replace("{{summary}}", &summary.stat)
        .replace("{{diffStat}}", &summary.stat)
        .replace("{{files}}", &summary.files.join(", "))
}

/// Parse `git diff --stat` + `git status --porcelain` output into a summary.
/// Pure so it is unit-testable without invoking git.
pub fn build_summary(diff_stat: &str, porcelain: &str) -> GitSummary {
    let files: Vec<String> = porcelain
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| l[3..].trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // Hash: cheap FNV-1a over the porcelain (order-stable from git).
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in porcelain.trim().bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    GitSummary {
        stat: diff_stat.trim().to_string(),
        changed: !files.is_empty(),
        hash: if files.is_empty() {
            String::new()
        } else {
            format!("{hash:016x}")
        },
        files,
    }
}

#[derive(Default)]
pub struct AutomationState {
    pub rules: Vec<Rule>,
    runtimes: HashMap<String, RuleRuntime>,
    pub pending: HashMap<String, Proposal>,
}

impl AutomationState {
    pub fn with_rules(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            ..Default::default()
        }
    }

    pub fn runtime_mut(&mut self, rule_id: &str) -> &mut RuleRuntime {
        self.runtimes.entry(rule_id.to_string()).or_default()
    }

    /// Drop runtime/pending state for rules that no longer exist.
    pub fn gc(&mut self) {
        let ids: std::collections::HashSet<&str> =
            self.rules.iter().map(|r| r.id.as_str()).collect();
        self.runtimes.retain(|k, _| ids.contains(k.as_str()));
        self.pending.retain(|_, p| ids.contains(p.rule_id.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> Rule {
        Rule {
            id: "r1".into(),
            name: "git review".into(),
            enabled: true,
            repo: "C:/repo".into(),
            source: None,
            cooldown_ms: 5000,
            max_per_min: 4,
            target_label: Some("codex".into()),
            target_pane: None,
            template: "changed: {{files}}\n{{diffStat}}".into(),
            submit: true,
            require_idle: true,
            mode: RuleMode::Confirm,
        }
    }

    fn is_fire(d: &Decision) -> bool {
        matches!(d, Decision::Fire)
    }

    #[test]
    fn debounce_requires_two_stable_polls() {
        let mut rt = RuleRuntime::default();
        let r = rule();
        let t = Instant::now();
        // first sight of hash A -> settle
        assert!(!is_fire(&rt.decide(&r, Some("A"), t, false)));
        // second poll, same hash -> fires
        assert!(is_fire(&rt.decide(&r, Some("A"), t, false)));
    }

    #[test]
    fn no_changes_never_fires() {
        let mut rt = RuleRuntime::default();
        let r = rule();
        let t = Instant::now();
        assert!(!is_fire(&rt.decide(&r, None, t, false)));
        assert!(!is_fire(&rt.decide(&r, Some(""), t, false)));
    }

    #[test]
    fn dedup_same_diff_after_fire() {
        let mut rt = RuleRuntime::default();
        let r = rule();
        let t = Instant::now();
        rt.decide(&r, Some("A"), t, false);
        assert!(is_fire(&rt.decide(&r, Some("A"), t, false)));
        rt.record_fire(Some("A"), t);
        // same diff still present next polls -> deduped
        rt.decide(&r, Some("A"), t, false);
        assert!(!is_fire(&rt.decide(&r, Some("A"), t, false)));
    }

    #[test]
    fn cooldown_blocks_new_diff_until_elapsed() {
        let mut rt = RuleRuntime::default();
        let r = rule();
        let t0 = Instant::now();
        rt.decide(&r, Some("A"), t0, false);
        rt.decide(&r, Some("A"), t0, false);
        rt.record_fire(Some("A"), t0);
        // new diff B arrives within cooldown
        let t1 = t0 + Duration::from_millis(1000);
        rt.decide(&r, Some("B"), t1, false); // settle
        match rt.decide(&r, Some("B"), t1, false) {
            Decision::Skip(reason) => assert_eq!(reason, "cooldown"),
            Decision::Fire => panic!("should be in cooldown"),
        }
        // after cooldown, B fires
        let t2 = t0 + Duration::from_millis(6000);
        rt.decide(&r, Some("B"), t2, false);
        assert!(is_fire(&rt.decide(&r, Some("B"), t2, false)));
    }

    #[test]
    fn rate_limit_caps_fires_per_minute() {
        let mut r = rule();
        r.cooldown_ms = 0;
        r.max_per_min = 2;
        let mut rt = RuleRuntime::default();
        let base = Instant::now();
        for i in 0..2 {
            let t = base + Duration::from_secs(i * 5);
            let h = format!("h{i}");
            rt.decide(&r, Some(&h), t, false);
            assert!(is_fire(&rt.decide(&r, Some(&h), t, false)));
            rt.record_fire(Some(&h), t);
        }
        // third distinct diff within the window -> rate limited
        let t = base + Duration::from_secs(11);
        rt.decide(&r, Some("h2"), t, false);
        match rt.decide(&r, Some("h2"), t, false) {
            Decision::Skip(reason) => assert_eq!(reason, "rate limited"),
            Decision::Fire => panic!("should be rate limited"),
        }
    }

    #[test]
    fn manual_bypasses_all_gates() {
        let mut rt = RuleRuntime::default();
        let r = rule();
        let t = Instant::now();
        // even with no changes, manual fires
        assert!(is_fire(&rt.decide(&r, None, t, true)));
    }

    #[test]
    fn effective_source_falls_back_to_repo() {
        let r = rule(); // source: None, repo set
        assert_eq!(
            r.effective_source(),
            RuleSource::GitDiff {
                repo: "C:/repo".into()
            }
        );
    }

    #[test]
    fn timer_fires_after_interval_then_repeats() {
        let mut r = rule();
        r.source = Some(RuleSource::Timer { every_ms: 10_000 });
        r.cooldown_ms = 0;
        let mut rt = RuleRuntime::default();
        let t0 = Instant::now();
        // first observation only arms the anchor
        assert!(!is_fire(&rt.decide_timer(&r, 10_000, t0, false)));
        // before the interval elapses
        assert!(!is_fire(&rt.decide_timer(&r, 10_000, t0 + Duration::from_secs(5), false)));
        // after the interval
        let t1 = t0 + Duration::from_secs(10);
        assert!(is_fire(&rt.decide_timer(&r, 10_000, t1, false)));
        rt.record_fire(None, t1);
        // not again until the next interval
        assert!(!is_fire(&rt.decide_timer(&r, 10_000, t1 + Duration::from_secs(5), false)));
        let t2 = t1 + Duration::from_secs(10);
        assert!(is_fire(&rt.decide_timer(&r, 10_000, t2, false)));
    }

    #[test]
    fn timer_validate_rejects_tiny_interval() {
        let mut r = rule();
        r.source = Some(RuleSource::Timer { every_ms: 100 });
        assert!(r.validate().is_err());
        r.source = Some(RuleSource::Timer { every_ms: 60_000 });
        assert!(r.validate().is_ok());
    }

    #[test]
    fn rule_source_wire_format() {
        // Lock the camelCase wire shape the frontend sends.
        let timer: RuleSource = serde_json::from_str(r#"{"type":"timer","everyMs":60000}"#).unwrap();
        assert_eq!(timer, RuleSource::Timer { every_ms: 60000 });
        let git: RuleSource =
            serde_json::from_str(r#"{"type":"gitDiff","repo":"C:/r"}"#).unwrap();
        assert_eq!(git, RuleSource::GitDiff { repo: "C:/r".into() });
        // and back out
        let v = serde_json::to_value(RuleSource::Timer { every_ms: 5000 }).unwrap();
        assert_eq!(v["type"], "timer");
        assert_eq!(v["everyMs"], 5000);
    }

    #[test]
    fn timer_manual_fires_immediately() {
        let mut r = rule();
        r.source = Some(RuleSource::Timer { every_ms: 60_000 });
        let mut rt = RuleRuntime::default();
        assert!(is_fire(&rt.decide_timer(&r, 60_000, Instant::now(), true)));
    }

    #[test]
    fn summary_parsing_and_hash_stability() {
        let porcelain = " M src/main.rs\n?? new.txt\n";
        let s1 = build_summary("1 file changed", porcelain);
        assert!(s1.changed);
        assert_eq!(s1.files, vec!["src/main.rs", "new.txt"]);
        let s2 = build_summary("different stat text", porcelain);
        assert_eq!(s1.hash, s2.hash, "hash tracks working tree, not stat text");
        let s3 = build_summary("x", " M src/other.rs\n");
        assert_ne!(s1.hash, s3.hash);
        let empty = build_summary("", "");
        assert!(!empty.changed);
        assert!(empty.hash.is_empty());
    }

    #[test]
    fn template_render() {
        let s = build_summary("2 files changed, 3 insertions", " M a.rs\n M b.rs\n");
        let out = render_template("Review {{files}} — {{diffStat}}", &s);
        assert_eq!(out, "Review a.rs, b.rs — 2 files changed, 3 insertions");
    }

    #[test]
    fn gc_drops_orphan_state() {
        let mut st = AutomationState {
            rules: vec![rule()],
            ..Default::default()
        };
        st.runtime_mut("r1");
        st.runtime_mut("gone");
        st.pending.insert(
            "p1".into(),
            Proposal {
                id: "p1".into(),
                rule_id: "gone".into(),
                rule_name: "x".into(),
                target_label: None,
                target_pane: None,
                text: "t".into(),
                submit: true,
                require_idle: true,
                summary: "s".into(),
            },
        );
        st.gc();
        assert!(st.runtimes.contains_key("r1"));
        assert!(!st.runtimes.contains_key("gone"));
        assert!(st.pending.is_empty());
    }
}
