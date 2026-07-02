# Example templates (Phase B)

A **template** is a preset pane layout that materializes a fresh workspace in
one action (roadmap §3, `docs/ADR-009-project-templates.md`).

## Try one

1. Copy `ai-pair-dev.json` into your templates dir:
   `%APPDATA%\com.terminalf.app\templates\` (create it if missing).
2. In terminal-f: command palette → `Template: Apply "ai-pair-dev"`.
3. Answer the `repo` prompt with a folder path.

You get a new workspace: a **claude** pane (starts `claude`), a plain **shell**
pane, and a **codex** pane (injection + observation enabled, runs
`git status`).

## Fields

- `params`: `${name}` placeholders the user fills at apply time; `${env:VAR}`
  reads an environment variable. `default` pre-fills; `kind` (`folder`/`text`)
  is a UI hint.
- `kind: "pane"`: `cwd`, `labels`, `allowInjection`, `allowObserve`, `command`
  (replaces the shell), `startupCommand` (typed into the shell once ready — the
  pane stays a shell afterwards).
- `kind: "split"`: `direction` (`row`/`col`), `ratio` (0.1–0.9), `first`,
  `second`.

## Repo-local profiles

Put the same JSON at `<repo>/.terminal-f/profile.json` and use
`Template: Apply repo profile`. A profile that runs commands requires a
one-time **trust** confirmation before it can auto-run (workspace trust).
