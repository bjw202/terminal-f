//! PowerShell shell-integration snippets (opt-in, user-profile edits).
//!
//! Two independent, fenced blocks we can add to the pwsh `$PROFILE`:
//!
//! - **multiline** (ADR-010 follow-up): terminal-f sends ESC+CR (`\x1b\r`) for
//!   Ctrl+Enter / Shift+Enter. pwsh receives that as **Alt+Enter**, unbound by
//!   default. Binding Alt+Enter to PSReadLine's `AddLine` makes the chord
//!   insert a newline — the pwsh-native `Shift+Enter = AddLine` behavior,
//!   reached through a sequence a VT terminal can actually transmit.
//!
//! - **cwd** (ADR-011): a prompt wrapper that emits `OSC 9;9;<path>` each
//!   prompt so the backend can track the live directory and open a split there.
//!
//! This module owns only the *pure* snippet text and idempotent
//! append/detect/remove logic so it can be unit-tested without touching a real
//! profile. Path resolution and file IO live in commands.rs.

pub const MULTILINE_BEGIN: &str = "# >>> terminal-f multiline >>>";
pub const MULTILINE_END: &str = "# <<< terminal-f multiline <<<";
pub const CWD_BEGIN: &str = "# >>> terminal-f cwd >>>";
pub const CWD_END: &str = "# <<< terminal-f cwd <<<";

/// The multiline block: bind Alt+Enter -> AddLine. No-op without PSReadLine.
pub fn multiline_snippet() -> String {
    format!(
        "{MULTILINE_BEGIN}\n\
         # terminal-f: Ctrl+Enter / Shift+Enter insert a newline instead of\n\
         # submitting. The terminal sends Alt+Enter; bind it to AddLine.\n\
         # Remove this block (these fenced lines) to undo.\n\
         if (Get-Module -ListAvailable PSReadLine) {{\n\
         \x20   Import-Module PSReadLine -ErrorAction SilentlyContinue\n\
         \x20   Set-PSReadLineKeyHandler -Chord 'Alt+Enter' -Function AddLine -ErrorAction SilentlyContinue\n\
         }}\n\
         {MULTILINE_END}\n"
    )
}

/// The cwd block: wrap the prompt to emit OSC 9;9 with the current directory so
/// a split opens where the user is. Chains any existing prompt (oh-my-posh,
/// starship, …) by preserving it as `__termf_orig_prompt`.
pub fn cwd_snippet() -> String {
    format!(
        "{CWD_BEGIN}\n\
         # terminal-f: report the working directory via OSC 9;9 each prompt so a\n\
         # new split/tab opens in the live directory. The OSC is prepended to the\n\
         # prompt's return string (the way Windows Terminal / VS Code do it) — a\n\
         # Console write inside prompt does not reliably reach the terminal under\n\
         # PSReadLine. Wraps any existing prompt. Remove this block to undo.\n\
         if (-not (Test-Path Function:\\__termf_orig_prompt)) {{\n\
         \x20   Copy-Item Function:\\prompt Function:\\__termf_orig_prompt -ErrorAction SilentlyContinue\n\
         }}\n\
         function prompt {{\n\
         \x20   $tf_osc = ''\n\
         \x20   try {{ $tf_p = $PWD.ProviderPath; if ($tf_p) {{ $tf_osc = \"$([char]27)]9;9;$tf_p$([char]27)\\\" }} }} catch {{}}\n\
         \x20   $tf_base = if (Test-Path Function:\\__termf_orig_prompt) {{ & __termf_orig_prompt }} else {{ \"PS $($PWD.Path)> \" }}\n\
         \x20   return ($tf_osc + $tf_base)\n\
         }}\n\
         {CWD_END}\n"
    )
}

/// True if a fenced block (identified by its begin marker) is present.
pub fn is_installed(profile: &str, begin: &str) -> bool {
    profile.contains(begin)
}

/// Append `snippet` to `profile` unless already present (idempotent), keeping a
/// blank-line separator from prior content.
pub fn with_block(profile: &str, snippet: &str, begin: &str) -> String {
    if is_installed(profile, begin) {
        return profile.to_string();
    }
    let mut out = String::from(profile);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n'); // blank line before our block
    }
    out.push_str(snippet);
    out
}

/// Remove a fenced block (begin..=end) from `profile` (idempotent), also
/// swallowing the blank line we inserted before it and the newline after it.
pub fn without_block(profile: &str, begin: &str, end: &str) -> String {
    let Some(start) = profile.find(begin) else {
        return profile.to_string();
    };
    let Some(end_rel) = profile[start..].find(end) else {
        return profile.to_string();
    };
    let end_idx = start + end_rel + end.len();
    let mut tail_start = end_idx;
    if profile[tail_start..].starts_with('\n') {
        tail_start += 1;
    }
    let mut head_end = start;
    if profile[..head_end].ends_with("\n\n") {
        head_end -= 1;
    }
    format!("{}{}", &profile[..head_end], &profile[tail_start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_snippet_binds_altenter() {
        let s = multiline_snippet();
        assert!(s.starts_with(MULTILINE_BEGIN));
        assert!(s.trim_end().ends_with(MULTILINE_END));
        assert!(s.contains("Set-PSReadLineKeyHandler -Chord 'Alt+Enter' -Function AddLine"));
    }

    #[test]
    fn cwd_snippet_emits_osc99_and_chains_prompt() {
        let s = cwd_snippet();
        assert!(s.starts_with(CWD_BEGIN));
        assert!(s.trim_end().ends_with(CWD_END));
        assert!(s.contains("]9;9;"));
        assert!(s.contains("__termf_orig_prompt"));
    }

    #[test]
    fn append_is_idempotent() {
        let base = "# my profile\nSet-Alias ll Get-ChildItem\n";
        let once = with_block(base, &multiline_snippet(), MULTILINE_BEGIN);
        assert!(is_installed(&once, MULTILINE_BEGIN));
        assert!(once.contains("# my profile"));
        let twice = with_block(&once, &multiline_snippet(), MULTILINE_BEGIN);
        assert_eq!(once, twice, "second install must be a no-op");
        assert_eq!(once.matches(MULTILINE_BEGIN).count(), 1);
    }

    #[test]
    fn both_blocks_coexist_and_remove_independently() {
        let base = "# profile\n";
        let a = with_block(base, &multiline_snippet(), MULTILINE_BEGIN);
        let b = with_block(&a, &cwd_snippet(), CWD_BEGIN);
        assert!(is_installed(&b, MULTILINE_BEGIN));
        assert!(is_installed(&b, CWD_BEGIN));
        // remove cwd only; multiline stays
        let c = without_block(&b, CWD_BEGIN, CWD_END);
        assert!(is_installed(&c, MULTILINE_BEGIN));
        assert!(!is_installed(&c, CWD_BEGIN));
    }

    #[test]
    fn append_to_empty_profile() {
        let out = with_block("", &cwd_snippet(), CWD_BEGIN);
        assert!(is_installed(&out, CWD_BEGIN));
        assert!(out.starts_with(CWD_BEGIN));
    }

    #[test]
    fn remove_restores_original() {
        let base = "# my profile\nSet-Alias ll Get-ChildItem\n";
        let installed = with_block(base, &multiline_snippet(), MULTILINE_BEGIN);
        let removed = without_block(&installed, MULTILINE_BEGIN, MULTILINE_END);
        assert!(!is_installed(&removed, MULTILINE_BEGIN));
        assert_eq!(removed, base);
    }

    #[test]
    fn remove_is_noop_when_absent() {
        let base = "# nothing here\n";
        assert_eq!(without_block(base, MULTILINE_BEGIN, MULTILINE_END), base);
    }
}
