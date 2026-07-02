// Reference control-API broker: git-diff → claude -p review → inject (M2.2).
//
// Watches a git repo; when the working tree changes and settles, asks
// `claude -p` (headless) to review the diff, then injects the review request
// (or the review itself) into a labeled terminal-f pane via the control API.
//
// This is a REFERENCE/EXAMPLE, not production code. It demonstrates the
// intended shape: judgment (the LLM call) lives OUT of process; terminal-f is
// only "eyes and hands" reached over the named pipe. Every injection still
// passes terminal-f's backend gates (allowlist, idle, audit).
//
// Usage:
//   node index.mjs --repo C:\path\to\repo --label codex [--auto] [--no-ai]
//
// Requires: Node 18+, terminal-f running (writes control-api.json), a pane
// labeled <label> with injection enabled. `claude` on PATH unless --no-ai.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { connect } from "node:net";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileP = promisify(execFile);
const args = parseArgs(process.argv.slice(2));
const repo = args.repo ?? process.cwd();
const label = args.label ?? "codex";
const useAi = !args["no-ai"];
const POLL_MS = 4000;

// terminal-f writes control-api.json (pipe name + token) next to its config.
// Default location on Windows: %APPDATA%\com.terminalf.app\control-api.json
function loadControlApi() {
  const p =
    args.info ??
    join(process.env.APPDATA ?? join(homedir(), "AppData", "Roaming"), "com.terminalf.app", "control-api.json");
  const { pipeName, token } = JSON.parse(readFileSync(p, "utf8"));
  return { pipeName, token };
}

// interprocess GenericNamespaced maps <name> to \\.\pipe\<name> on Windows.
function pipePath(pipeName) {
  return `\\\\.\\pipe\\${pipeName}`;
}

// Minimal newline-delimited JSON-RPC client over the pipe.
function makeClient(pipeName) {
  const sock = connect(pipePath(pipeName));
  let buf = "";
  const waiters = [];
  sock.setEncoding("utf8");
  sock.on("data", (chunk) => {
    buf += chunk;
    let nl;
    while ((nl = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, nl);
      buf = buf.slice(nl + 1);
      const w = waiters.shift();
      if (w) w(JSON.parse(line));
    }
  });
  const ready = new Promise((res, rej) => {
    sock.once("connect", res);
    sock.once("error", rej);
  });
  let id = 0;
  const call = (method, params = {}) =>
    new Promise((resolve, reject) => {
      waiters.push((resp) => (resp.ok ? resolve(resp.result) : reject(new Error(resp.error))));
      sock.write(JSON.stringify({ id: ++id, method, params }) + "\n");
    });
  return { ready, call, close: () => sock.end() };
}

async function gitStat(cwd) {
  try {
    const { stdout: porcelain } = await execFileP("git", ["-C", cwd, "status", "--porcelain"]);
    const { stdout: stat } = await execFileP("git", ["-C", cwd, "diff", "--stat"]);
    return { porcelain: porcelain.trim(), stat: stat.trim() };
  } catch (e) {
    console.error("[broker] git error:", e.message);
    return { porcelain: "", stat: "" };
  }
}

async function aiReview(cwd, stat) {
  if (!useAi) return `Please review the working-tree changes:\n${stat}`;
  try {
    const { stdout: diff } = await execFileP("git", ["-C", cwd, "diff"], { maxBuffer: 4 * 1024 * 1024 });
    const prompt =
      "You are reviewing a git working-tree diff. In 3-5 bullet points, list " +
      "the most important things to check or fix. Diff:\n\n" +
      diff.slice(0, 12000);
    // Headless Claude Code: one-shot print mode, JSON output for parsing.
    const { stdout } = await execFileP("claude", ["-p", prompt, "--output-format", "json"], {
      maxBuffer: 8 * 1024 * 1024,
    });
    const parsed = JSON.parse(stdout);
    const text = parsed.result ?? parsed.text ?? stdout;
    return `Automated review of your changes:\n${text}`;
  } catch (e) {
    console.error("[broker] claude -p failed, falling back to plain prompt:", e.message);
    return `Please review the working-tree changes:\n${stat}`;
  }
}

async function main() {
  const { pipeName, token } = loadControlApi();
  const client = makeClient(pipeName);
  await client.ready;
  await client.call("auth", { token, client: "git-review-broker" });
  console.log(`[broker] connected; watching ${repo}, target label "${label}"`);

  const panes = await client.call("listPanes");
  const target = panes.panes.find((p) => p.labels.includes(label));
  if (!target) console.warn(`[broker] no pane labeled "${label}" yet (will retry each cycle)`);
  if (target && !target.allowInjection) {
    console.warn(`[broker] pane "${label}" does not allow injection — enable it in terminal-f`);
  }

  let lastHash = "";
  let stable = "";
  setInterval(async () => {
    try {
      const { porcelain, stat } = await gitStat(repo);
      if (!porcelain) return;
      // debounce: same porcelain across two ticks before acting
      if (porcelain !== stable) {
        stable = porcelain;
        return;
      }
      if (porcelain === lastHash) return;
      lastHash = porcelain;

      const text = await aiReview(repo, stat);
      const receipt = await client.call("injectPrompt", { label, text, submit: true });
      console.log(`[broker] injected ${receipt.bytes} bytes into "${label}"`);
    } catch (e) {
      console.error("[broker] cycle error:", e.message);
    }
  }, POLL_MS);
}

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next && !next.startsWith("--")) {
        out[key] = next;
        i++;
      } else {
        out[key] = true;
      }
    }
  }
  return out;
}

main().catch((e) => {
  console.error("[broker] fatal:", e);
  process.exit(1);
});
