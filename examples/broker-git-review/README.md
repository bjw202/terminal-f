# git-review broker (reference)

A minimal external broker for terminal-f's control API (M2.2). It shows the
intended pattern: **judgment lives out of process** (a headless `claude -p`
call), and terminal-f is reached only over the named pipe as "eyes and hands".

## What it does

1. Reads `control-api.json` (pipe name + auth token) that terminal-f writes at
   startup (default `%APPDATA%\com.terminalf.app\control-api.json`).
2. Connects to the named pipe, authenticates with the token.
3. Polls a git repo; when the working tree changes and settles, calls
   `claude -p "<review prompt>" --output-format json` to get a review.
4. Injects the review into a labeled pane via `injectPrompt` — which passes
   terminal-f's backend gates (per-pane injection allowlist, idle gate, audit).

## Prerequisites

- terminal-f running.
- A pane with a **label** (palette: *Pane: Edit labels*) — e.g. `codex`.
- That pane has **injection enabled** (palette: *Injection: Allow/disallow on
  focused pane*, shows ⚡).
- Node 18+. `claude` on PATH (or pass `--no-ai` to inject a plain prompt).

## Run

```powershell
node index.mjs --repo C:\path\to\repo --label codex
node index.mjs --repo C:\path\to\repo --label codex --no-ai   # skip the LLM call
```

## Notes

- This is example code, intentionally small (no reconnect/backoff, single
  target). It is not shipped or run by terminal-f.
- The control API is request/response; this broker polls `injectPrompt`. To
  react to a pane's *output* instead of git, mark the pane **observable**
  (palette: *Observe: …*) and poll `readOutput` with an advancing `from`
  byte offset.
- Security: the token in `control-api.json` is the access boundary. Anyone
  who can read that file (your user) can drive the pipe — every injection is
  still gated per-pane and written to terminal-f's audit log with
  `source = pipe:git-review-broker`.
