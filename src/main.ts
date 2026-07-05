// terminal-f frontend orchestrator.
//
// Responsibilities (spec 2): layout rendering, focused pane state, xterm
// visual state, and calling backend commands. The backend owns PTY sessions
// and the pane/workspace trees; nothing here mutates layout locally.

import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import * as ipc from "./ipc";
import * as terms from "./terms";
import { renderTree, type RenderCtx } from "./renderer";
import { renderSidebar, sidebarBusy } from "./sidebar";
import { registerCommandProvider } from "./commands";
import { isPaletteOpen, openPalette } from "./palette";
import { setTheme, themeById, THEMES } from "./themes";
import { confirmModal, listModal, promptModal } from "./modal";
import {
  collectLeaves,
  firstPaneId,
  type Direction,
  type PaneId,
  type Rule,
  type SessionInfo,
  type UiPrefs,
  type Workspace,
  type WorkspaceActivity,
  type WorkspaceMeta,
} from "./types";
import { runAutotest, type AutotestCtl } from "./autotest";

const sidebarEl = document.getElementById("sidebar")!;
const panesEl = document.getElementById("panes")!;
const statusEl = document.getElementById("statusbar")!;

let metas: WorkspaceMeta[] = [];
let activity = new Map<string, WorkspaceActivity>();
const sessionInfoByPane = new Map<string, SessionInfo>();
let defaultShellName = "shell";
let current: Workspace | null = null;
let activePaneId: PaneId | null = null;
let zoomedPaneId: PaneId | null = null;
let lastSwitchMs = 0;
let statusTimer: number | undefined;
let uiPrefs: UiPrefs = {};
let uiSaveTimer: number | undefined;

// ------------------------------------------------------------------ status

function showStatus(msg: string, isError = false): void {
  statusEl.textContent = msg;
  statusEl.classList.toggle("error", isError);
  statusEl.classList.add("visible");
  if (statusTimer) window.clearTimeout(statusTimer);
  statusTimer = window.setTimeout(() => statusEl.classList.remove("visible"), 4000);
}

function showWarnings(warnings: string[]): void {
  if (warnings.length > 0) showStatus(warnings.join(" | "), true);
}

// ---------------------------------------------------------------- ui prefs

function saveUiPrefs(): void {
  if (uiSaveTimer) window.clearTimeout(uiSaveTimer);
  uiSaveTimer = window.setTimeout(() => {
    void ipc.setUiPrefs(uiPrefs).catch((e) => console.warn("[setUiPrefs]", e));
  }, 300);
}

function applyThemeById(id: string): void {
  setTheme(themeById(id));
  terms.applyTerminalOptions({});
  uiPrefs.theme = id;
  saveUiPrefs();
}

function setFontSize(size: number): void {
  terms.applyTerminalOptions({ fontSize: size });
  uiPrefs.fontSize = terms.currentFontSize();
  saveUiPrefs();
}

function toggleCopyOnSelect(): void {
  const next = !(uiPrefs.copyOnSelect === true);
  uiPrefs.copyOnSelect = next;
  terms.setCopyOnSelect(next);
  saveUiPrefs();
  showStatus(`Copy-on-select ${next ? "enabled" : "disabled"}`);
}

// Opt-in pwsh $PROFILE shell integration. Both features edit the user's profile,
// so we show the exact snippet and confirm first, then require a fresh pane.
//   - "multiline": Ctrl/Shift+Enter reach pwsh as Alt+Enter (unbound by
//     default); bind Alt+Enter -> AddLine so the chord inserts a newline.
//   - "cwd": a prompt wrapper emits OSC 9;9 so a split opens in the live dir.
async function installShellIntegration(
  feature: ipc.ShellIntegrationFeature,
  labels: { title: string; what: string; use: string },
): Promise<void> {
  let info;
  try {
    info = await ipc.pwshIntegrationStatus(feature);
  } catch (e) {
    showStatus(String(e), true);
    return;
  }
  if (!info.available) {
    showStatus("PowerShell (pwsh) not found — this only applies to PowerShell.", true);
    return;
  }
  // Already present AND current — nothing to do but remind how to use/remove it.
  if (info.installed && info.upToDate) {
    listModal(`${labels.title} — already installed`, [
      `Profile: ${info.profilePath}`,
      "",
      labels.use,
      "Open a NEW PowerShell pane to pick it up (profiles load at shell start).",
      "To undo, delete the fenced terminal-f block from the profile above.",
    ]);
    return;
  }
  // Installed but an OLDER block is present → offer to refresh it in place.
  const updating = info.installed; // implies !info.upToDate here
  const ok = await confirmModal({
    title: updating ? `Update ${labels.title}` : labels.title,
    okLabel: updating ? "Update" : "Install",
    body: [
      updating
        ? "An older version of this block is in your profile. This replaces it with the current one:"
        : labels.what,
      "",
      `  ${info.profilePath}`,
      "",
      ...info.snippet.split("\n").map((l) => (l ? `    ${l}` : "")),
      "Open a NEW PowerShell pane afterwards to pick it up.",
    ],
  });
  restoreTermFocus();
  if (!ok) return;
  try {
    const res = await ipc.installPwshIntegration(feature);
    showStatus(
      `${updating ? "Updated" : "Installed"}. Open a NEW PowerShell pane to use it. (${res.profilePath})`,
    );
  } catch (e) {
    showStatus(String(e), true);
  }
}

// ------------------------------------------------------------------ render

const renderCtx: RenderCtx = {
  getPaneEl(paneId: string): HTMLElement {
    return terms.getOrCreateView(paneId, {
      onFocusRequest: focusPane,
      onCloseRequest: (id) => void closePaneById(id),
      onZoomRequest: (id) => {
        focusPane(id);
        toggleZoom();
      },
      isShortcut: isShortcutEvent,
    }).el;
  },
  commitRatio(splitId: string, ratio: number): Promise<number> {
    if (!current) return Promise.reject("no workspace");
    return ipc.resizeSplit(current.id, splitId, ratio).then((clamped) => {
      refitAll();
      return clamped;
    });
  },
};

function renderWorkspace(): void {
  if (!current) return;
  if (zoomedPaneId && terms.getView(zoomedPaneId)) {
    // Zoom: render only the zoomed pane; the tree itself is untouched.
    const cell = document.createElement("div");
    cell.className = "pane-cell zoomed";
    cell.appendChild(terms.getView(zoomedPaneId)!.el);
    panesEl.replaceChildren(cell);
  } else {
    zoomedPaneId = null;
    renderTree(panesEl, current.root, renderCtx);
  }
  requestAnimationFrame(refitAll);
  updateFocusStyles();
}

function refitAll(): void {
  for (const view of terms.allViews()) terms.syncSize(view);
}

function refreshSidebar(): void {
  if (sidebarBusy) return;
  renderSidebar(sidebarEl, {
    metas,
    activity,
    currentId: current?.id ?? null,
    collapsed: uiPrefs.sidebar?.collapsed ?? false,
    width: uiPrefs.sidebar?.width ?? 220,
    onSwitch: (id) => void switchTo(id),
    onCreate: () => void addWorkspace(),
    onDelete: (id) => void removeWorkspace(id),
    onRename: (id, name) => {
      void ipc
        .renameWorkspace(id, name)
        .then((m) => {
          metas = m;
          refreshSidebar();
        })
        .catch((e) => {
          showStatus(String(e), true);
          refreshSidebar();
        });
    },
    onReorder: (ids) => {
      void ipc
        .reorderWorkspaces(ids)
        .then((m) => {
          metas = m;
          refreshSidebar();
        })
        .catch((e) => showStatus(String(e), true));
    },
    onSetColor: (id, color) => {
      void ipc
        .setWorkspaceColor(id, color)
        .then((m) => {
          metas = m;
          refreshSidebar();
        })
        .catch((e) => showStatus(String(e), true));
    },
    onToggle: toggleSidebar,
    onWidthChange: (w) => {
      uiPrefs.sidebar = { ...uiPrefs.sidebar, width: w };
      saveUiPrefs();
      refreshSidebar();
    },
    onOpenPalette: () => openPalette(restoreTermFocus),
  });
}

function toggleSidebar(): void {
  uiPrefs.sidebar = {
    ...uiPrefs.sidebar,
    collapsed: !(uiPrefs.sidebar?.collapsed ?? false),
  };
  saveUiPrefs();
  refreshSidebar();
  requestAnimationFrame(refitAll);
}

// ------------------------------------------------------------------ focus

function focusPane(paneId: PaneId): void {
  if (!current) return;
  activePaneId = paneId;
  updateFocusStyles();
  terms.getView(paneId)?.term.focus();
  void ipc.setActivePane(current.id, paneId).catch(() => {});
}

function restoreTermFocus(): void {
  if (activePaneId) terms.getView(activePaneId)?.term.focus();
}

function updateFocusStyles(): void {
  for (const view of terms.allViews()) {
    view.el.classList.toggle("focused", view.paneId === activePaneId);
  }
}

// -------------------------------------------------------------- pane mount

async function mountPane(paneId: PaneId): Promise<void> {
  const view = terms.getView(paneId);
  if (!view) return;
  const snap = terms.snapshots.get(paneId);
  if (snap?.data) view.term.write(snap.data); // 5. restore visual snapshot
  view.lastSeq = snap?.lastSeq ?? 0;
  terms.snapshots.delete(paneId);
  try {
    // 6. replay pending backend output accumulated while unmounted
    const replay = await ipc.replayPane(paneId, view.lastSeq);
    if (replay.dropped) {
      view.term.write(
        "\r\n\x1b[33m[terminal-f: output overflow while inactive, oldest chunks dropped]\x1b[0m\r\n",
      );
    }
    if (replay.data) view.term.write(replay.data);
    view.lastSeq = replay.lastSeq;
    if (replay.state === "exited" && !view.exitShown) {
      view.exitShown = true;
      view.term.write("\r\n\x1b[31m[process exited]\x1b[0m\r\n");
    }
  } catch (e) {
    // Pane without a session (e.g. PTY cap reached): show why.
    view.term.write(`\r\n\x1b[31m[no session: ${String(e)}]\x1b[0m\r\n`);
  }
}

function noteExitedSessions(sessions: SessionInfo[]): void {
  for (const s of sessions) {
    sessionInfoByPane.set(s.paneId, s);
    const view = terms.getView(s.paneId);
    if (view && s.state === "exited" && !view.exitShown) {
      view.exitShown = true;
      view.term.write(`\r\n\x1b[31m[process exited (code ${s.exitCode ?? "?"})]\x1b[0m\r\n`);
    }
  }
  updateHeaders();
}

// ------------------------------------------------------------ pane headers

function programName(path: string): string {
  const base = path.split(/[\\/]/).pop() ?? path;
  return base.replace(/\.exe$/i, "");
}

/** Refresh the slim identification tab on every pane; the tabs hide
 * themselves (CSS) when the workspace has only one pane. */
function updateHeaders(): void {
  if (!current) return;
  const leaves = collectLeaves(current.root);
  panesEl.classList.toggle("single-pane", leaves.length === 1 && !zoomedPaneId);
  leaves.forEach((leaf, i) => {
    const info = sessionInfoByPane.get(leaf.id);
    const title = info
      ? programName(info.command)
      : leaf.command
        ? programName(leaf.command)
        : defaultShellName;
    terms.setPaneHeader(
      leaf.id,
      i + 1,
      title,
      info?.state === "exited",
      leaf.labels,
      leaf.allowInjection,
      leaf.allowObserve,
    );
  });
}

// --------------------------------------------------------- workspace switch

/**
 * Workspace switch sequence (spec 6.4):
 * 1. snapshot current workspace's xterms  2. unmount them (dispose)
 * 3. backend keeps PTY sessions alive     4. mount target xterms
 * 5. restore snapshots                    6. replay pending output
 * 7. restore focus
 */
async function switchTo(workspaceId: string): Promise<number> {
  const t0 = performance.now();
  zoomedPaneId = null;
  if (current) {
    for (const leaf of collectLeaves(current.root)) {
      terms.snapshotAndDispose(leaf.id);
    }
  }
  const res = await ipc.switchWorkspace(workspaceId);
  current = res.workspace;
  activePaneId =
    res.workspace.activePaneId ?? firstPaneId(res.workspace.root);
  renderWorkspace();
  sessionInfoByPane.clear();
  await Promise.all(collectLeaves(res.workspace.root).map((l) => mountPane(l.id)));
  noteExitedSessions(res.sessions);
  showWarnings(res.warnings);
  focusPane(activePaneId);
  refreshSidebar();
  lastSwitchMs = performance.now() - t0;
  console.log(`[terminal-f] workspace switch: ${lastSwitchMs.toFixed(1)}ms`);
  return lastSwitchMs;
}

async function addWorkspace(): Promise<string> {
  const res = await ipc.createWorkspace();
  metas = res.workspaces;
  if (res.warning) showStatus(res.warning, true);
  await switchTo(res.workspace.id);
  return res.workspace.id;
}

async function removeWorkspace(workspaceId: string): Promise<void> {
  const wasCurrent = current?.id === workspaceId;
  if (wasCurrent && current) {
    // The panes and sessions are going away; drop visual state entirely.
    for (const leaf of collectLeaves(current.root)) terms.dropPaneState(leaf.id);
    current = null;
  }
  try {
    const snap = await ipc.deleteWorkspace(workspaceId);
    metas = snap.workspaces;
    if (wasCurrent && snap.activeWorkspaceId) {
      await switchTo(snap.activeWorkspaceId);
    } else {
      refreshSidebar();
    }
  } catch (e) {
    showStatus(String(e), true);
  }
}

// ------------------------------------------------------------------- panes

async function splitActive(direction: Direction): Promise<void> {
  if (!current || !activePaneId) return;
  zoomedPaneId = null;
  try {
    const res = await ipc.splitPane(current.id, activePaneId, direction);
    current = res.workspace;
    const newPaneId = res.workspace.activePaneId ?? activePaneId;
    renderWorkspace();
    await mountPane(newPaneId);
    noteExitedSessions(res.sessions);
    showWarnings(res.warnings);
    focusPane(newPaneId);
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function closePaneById(paneId: PaneId): Promise<void> {
  if (!current) return;
  zoomedPaneId = null;
  try {
    const res = await ipc.closePane(current.id, paneId);
    terms.dropPaneState(paneId);
    sessionInfoByPane.delete(paneId);
    current = res.workspace;
    renderWorkspace();
    updateHeaders();
    focusPane(res.workspace.activePaneId ?? firstPaneId(res.workspace.root));
  } catch (e) {
    showStatus(String(e), true); // e.g. "cannot close the last pane"
  }
}

async function closeActive(): Promise<void> {
  if (activePaneId) await closePaneById(activePaneId);
}

function toggleZoom(): void {
  if (!current || !activePaneId) return;
  zoomedPaneId = zoomedPaneId ? null : activePaneId;
  renderWorkspace();
  updateHeaders();
  restoreTermFocus();
}

// --------------------------------------------------------------- shortcuts

// Ctrl+Shift+D: split row | Ctrl+Shift+-: split column | Ctrl+Shift+W: close
// Ctrl+Shift+P: palette   | Ctrl+Shift+Z: zoom pane    | Ctrl+Shift+B: sidebar
// Ctrl+1..8: switch workspace by position.
// Note: sidebar toggle is Ctrl+Shift+B (not the conventional Ctrl+B) because
// plain Ctrl+B belongs to shells (readline backward-char, tmux prefix) and
// must reach the PTY untouched.
function isShortcutEvent(e: KeyboardEvent): boolean {
  if (e.ctrlKey && e.shiftKey && !e.altKey) {
    return ["KeyD", "Minus", "KeyW", "KeyP", "KeyZ", "KeyB"].includes(e.code);
  }
  if (e.ctrlKey && !e.shiftKey && !e.altKey) {
    return /^Digit[1-8]$/.test(e.code);
  }
  return false;
}

window.addEventListener(
  "keydown",
  (e) => {
    if (isPaletteOpen() || !isShortcutEvent(e)) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.ctrlKey && e.shiftKey) {
      if (e.code === "KeyD") void splitActive("row");
      else if (e.code === "Minus") void splitActive("column");
      else if (e.code === "KeyW") void closeActive();
      else if (e.code === "KeyP") openPalette(restoreTermFocus);
      else if (e.code === "KeyZ") toggleZoom();
      else if (e.code === "KeyB") toggleSidebar();
    } else {
      const idx = Number(e.code.slice(5)) - 1;
      const target = metas[idx];
      if (target && target.id !== current?.id) void switchTo(target.id);
    }
  },
  { capture: true },
);

// ---------------------------------------------------------------- commands

registerCommandProvider(() => [
  { id: "pane.split.row", title: "Pane: Split left/right", hint: "Ctrl+Shift+D", run: () => splitActive("row") },
  { id: "pane.split.col", title: "Pane: Split top/bottom", hint: "Ctrl+Shift+-", run: () => splitActive("column") },
  { id: "pane.close", title: "Pane: Close focused", hint: "Ctrl+Shift+W", run: () => closeActive() },
  { id: "pane.zoom", title: "Pane: Toggle zoom", hint: "Ctrl+Shift+Z", run: () => toggleZoom() },
  { id: "ws.new", title: "Workspace: New", run: () => addWorkspace() },
  { id: "sidebar.toggle", title: "View: Toggle sidebar", hint: "Ctrl+Shift+B", run: () => toggleSidebar() },
  { id: "font.bigger", title: "View: Increase font size", run: () => setFontSize(terms.currentFontSize() + 1) },
  { id: "font.smaller", title: "View: Decrease font size", run: () => setFontSize(terms.currentFontSize() - 1) },
  {
    id: "copy.onSelect",
    title: `Copy: ${uiPrefs.copyOnSelect ? "Disable" : "Enable"} copy-on-select`,
    run: () => toggleCopyOnSelect(),
  },
  {
    id: "shell.pwshMultiline",
    title: "Shell: Enable multiline in PowerShell (Ctrl+Enter)",
    run: () =>
      installShellIntegration("multiline", {
        title: "Enable multiline input in PowerShell?",
        what: "This appends the following to your PowerShell profile so that Ctrl+Enter / Shift+Enter insert a newline instead of running the line:",
        use: "Ctrl+Enter / Shift+Enter insert a newline at the pwsh prompt.",
      }),
  },
  {
    id: "shell.pwshCwd",
    title: "Shell: Enable live directory tracking in PowerShell (split follows cwd)",
    run: () =>
      installShellIntegration("cwd", {
        title: "Enable live directory tracking in PowerShell?",
        what: "This appends the following to your PowerShell profile so that a new split opens in the directory you're currently in (not where the pane started):",
        use: "New splits open in the pane's current directory.",
      }),
  },
]);

// ------------------------------------------------------- injection (M2.0)

let injectionPaused = false;

function findLeaf(paneId: string) {
  return current ? collectLeaves(current.root).find((l) => l.id === paneId) : undefined;
}

function applyWorkspaceResult(res: { workspace: Workspace }): void {
  current = res.workspace;
  updateHeaders();
}

async function togglePaneInjection(): Promise<void> {
  if (!current || !activePaneId) return;
  const leaf = findLeaf(activePaneId);
  if (!leaf) return;
  try {
    const res = await ipc.setPaneInjection(current.id, activePaneId, !leaf.allowInjection);
    applyWorkspaceResult(res);
    showStatus(
      `Pane injection ${leaf.allowInjection ? "disabled" : "enabled"} (allowlist is per-pane, default off)`,
    );
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function togglePaneObserve(): Promise<void> {
  if (!current || !activePaneId) return;
  const leaf = findLeaf(activePaneId);
  if (!leaf) return;
  try {
    const res = await ipc.setPaneObserve(current.id, activePaneId, !leaf.allowObserve);
    applyWorkspaceResult(res);
    showStatus(
      `Pane observation ${leaf.allowObserve ? "disabled" : "enabled"} ` +
        `(control API can ${leaf.allowObserve ? "no longer" : "now"} read this pane's output)`,
    );
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function showControlApiInfo(): Promise<void> {
  try {
    const info = await ipc.controlApiInfo();
    listModal("Control API (named pipe)", [
      `pipe name : ${info.pipeName}`,
      `info file : ${info.infoPath}`,
      "",
      "A broker reads the info file (which also holds the auth token),",
      "connects to the pipe, sends {method:\"auth\",params:{token}}, then",
      "listPanes / readOutput / injectPrompt / listRules / runRule.",
      "Only panes with observation/injection enabled are reachable.",
    ]);
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function editPaneLabels(): Promise<void> {
  if (!current || !activePaneId) return;
  const leaf = findLeaf(activePaneId);
  if (!leaf) return;
  const result = await promptModal({
    title: "Pane labels (comma-separated, e.g. codex, reviewer)",
    initial: leaf.labels.join(", "),
    placeholder: "codex, reviewer",
  });
  restoreTermFocus();
  if (result === null) return;
  const labels = result.value.split(",").map((s) => s.trim()).filter(Boolean);
  try {
    applyWorkspaceResult(await ipc.setPaneLabels(current.id, activePaneId, labels));
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function injectIntoPane(paneId: string, paneNo: number): Promise<void> {
  const result = await promptModal({
    title: `Inject prompt into pane ${paneNo} (Ctrl+Enter to send)`,
    multiline: true,
    placeholder: "Text to type into the pane…",
    checkboxLabel: "submit (press Enter after the text)",
    checkboxInitial: true,
    okLabel: "Inject",
  });
  restoreTermFocus();
  if (result === null || !result.value) return;
  try {
    const receipt = await ipc.injectPrompt({
      paneId,
      text: result.value,
      submit: result.checked,
    });
    showStatus(
      `Injected ${receipt.bytes} bytes into pane ${paneNo}` +
        (receipt.bracketed ? " (bracketed paste)" : ""),
    );
  } catch (e) {
    showStatus(String(e), true); // busy / not allowed / paused
  }
}

async function showAuditLog(): Promise<void> {
  const entries = await ipc.readAudit(50).catch(() => []);
  listModal(
    "Injection audit log (last 50)",
    entries
      .map((a) => {
        const when = new Date(a.ts).toLocaleString();
        return `${when}  [${a.source}]  pane ${a.paneId.slice(0, 8)}…  ${a.bytes}B${a.submitted ? " ⏎" : ""}  "${a.preview}"`;
      })
      .reverse(),
  );
}

registerCommandProvider(() => [
  {
    id: "inject.togglePane",
    title: "Injection: Allow/disallow on focused pane",
    run: () => togglePaneInjection(),
  },
  { id: "inject.labels", title: "Pane: Edit labels (focused)", run: () => editPaneLabels() },
  {
    id: "observe.togglePane",
    title: "Observe: Allow/disallow output observation on focused pane",
    run: () => togglePaneObserve(),
  },
  {
    id: "control.info",
    title: "Control API: Show connection info (named pipe)",
    run: () => showControlApiInfo(),
  },
  {
    id: "inject.pause",
    title: injectionPaused
      ? "Injection: Resume (currently PAUSED)"
      : "Injection: Pause all (kill switch)",
    run: async () => {
      injectionPaused = await ipc.setInjectionPaused(!injectionPaused);
      showStatus(injectionPaused ? "Injection paused (kill switch ON)" : "Injection resumed");
    },
  },
  { id: "inject.audit", title: "Injection: Show audit log", run: () => showAuditLog() },
]);

registerCommandProvider(() => {
  if (!current) return [];
  return collectLeaves(current.root)
    .map((leaf, i) => ({ leaf, no: i + 1 }))
    .filter(({ leaf }) => leaf.allowInjection)
    .map(({ leaf, no }) => ({
      id: `inject.to.${leaf.id}`,
      title: `Injection: Send prompt to pane ${no}${leaf.labels.length ? ` [${leaf.labels.join(", ")}]` : ""}`,
      run: () => injectIntoPane(leaf.id, no),
    }));
});

// ---------------------------------------------------- automation (M2.1)

async function addGitReviewRule(): Promise<void> {
  if (!current || !activePaneId) return;
  const leaf = findLeaf(activePaneId);
  const repoDefault = leaf?.cwd ?? "";
  const repoRes = await promptModal({
    title: "Watch which git repo folder?",
    initial: repoDefault,
    placeholder: "C:\\path\\to\\repo",
  });
  if (repoRes === null || !repoRes.value.trim()) return;
  const labelRes = await promptModal({
    title: "Inject into pane with which label?",
    placeholder: "codex",
    checkboxLabel: "auto mode (inject without asking — leave off for confirm)",
    checkboxInitial: false,
  });
  restoreTermFocus();
  if (labelRes === null || !labelRes.value.trim()) return;
  const repo = repoRes.value.trim();
  const label = labelRes.value.trim().toLowerCase();
  const rule: Rule = {
    id: crypto.randomUUID(),
    name: `git review → ${label}`,
    enabled: true,
    repo,
    source: { type: "gitDiff", repo },
    cooldownMs: 5000,
    maxPerMin: 4,
    targetLabel: label,
    targetPane: null,
    template: "The working tree changed:\n{{diffStat}}\n\nPlease review the changes.",
    submit: true,
    requireIdle: true,
    mode: labelRes.checked ? "auto" : "confirm",
  };
  try {
    await ipc.upsertRule(rule);
    await refreshRuleCommands();
    showStatus(
      `Rule added (${rule.mode}); watching ${repo}. Target label "${label}" must be on an injection-enabled pane.`,
    );
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function addTimerRule(): Promise<void> {
  if (!current || !activePaneId) return;
  const minRes = await promptModal({
    title: "Timer: inject every how many minutes?",
    initial: "5",
    placeholder: "5",
  });
  if (minRes === null) return;
  const minutes = Number(minRes.value.trim());
  if (!Number.isFinite(minutes) || minutes <= 0) {
    restoreTermFocus();
    showStatus("Invalid interval", true);
    return;
  }
  const labelRes = await promptModal({
    title: "Inject into pane with which label?",
    placeholder: "codex",
    checkboxLabel: "auto mode (inject without asking — leave off for confirm)",
    checkboxInitial: false,
  });
  const textRes =
    labelRes && labelRes.value.trim()
      ? await promptModal({
          title: "Prompt to inject each interval",
          multiline: true,
          initial: "status?",
        })
      : null;
  restoreTermFocus();
  if (labelRes === null || !labelRes.value.trim() || textRes === null || !textRes.value.trim()) {
    return;
  }
  const label = labelRes.value.trim().toLowerCase();
  const rule: Rule = {
    id: crypto.randomUUID(),
    name: `timer ${minutes}m → ${label}`,
    enabled: true,
    repo: "",
    source: { type: "timer", everyMs: Math.round(minutes * 60_000) },
    cooldownMs: 0,
    maxPerMin: 4,
    targetLabel: label,
    targetPane: null,
    template: textRes.value,
    submit: true,
    requireIdle: true,
    mode: labelRes.checked ? "auto" : "confirm",
  };
  try {
    await ipc.upsertRule(rule);
    await refreshRuleCommands();
    showStatus(`Timer rule added: every ${minutes}m → "${label}" (${rule.mode})`);
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function manageRules(): Promise<void> {
  const rules = await ipc.listRules().catch(() => []);
  if (rules.length === 0) {
    showStatus("No automation rules. Use 'Automation: Add git-review rule'.");
    return;
  }
  listModal(
    "Automation rules",
    rules.map((r) => {
      const src = r.source
        ? r.source.type === "timer"
          ? `timer ${Math.round(r.source.everyMs / 60000)}m`
          : `git ${r.source.repo}`
        : `git ${r.repo}`;
      return `${r.enabled ? "●" : "○"} ${r.name}  [${r.mode}]  ${src}  →${r.targetLabel ?? r.targetPane ?? "?"}`;
    }),
  );
}

function proposalToast(p: import("./types").Proposal): void {
  document.getElementById(`proposal-${p.id}`)?.remove();
  const toast = document.createElement("div");
  toast.id = `proposal-${p.id}`;
  toast.className = "toast";
  const title = document.createElement("div");
  title.className = "toast-title";
  title.textContent = `Automation: ${p.ruleName}`;
  const body = document.createElement("div");
  body.className = "toast-body";
  const target = p.targetLabel ? `[${p.targetLabel}]` : (p.targetPane ?? "");
  body.textContent = `Inject into ${target}: "${p.text.slice(0, 120)}"`;
  const buttons = document.createElement("div");
  buttons.className = "toast-buttons";
  const approve = document.createElement("button");
  approve.className = "toast-btn primary";
  approve.textContent = "Approve";
  const dismiss = document.createElement("button");
  dismiss.className = "toast-btn";
  dismiss.textContent = "Dismiss";
  buttons.append(dismiss, approve);
  toast.append(title, body, buttons);
  document.getElementById("toasts")!.appendChild(toast);

  const close = () => toast.remove();
  approve.addEventListener("click", async () => {
    try {
      await ipc.resolveProposal(p.id, true);
      showStatus(`Injected via rule "${p.ruleName}"`);
    } catch (e) {
      showStatus(String(e), true); // busy/paused: proposal is re-queued
    }
    close();
  });
  dismiss.addEventListener("click", async () => {
    await ipc.resolveProposal(p.id, false).catch(() => {});
    close();
  });
}

void ipc.onAutomationProposal((p) => proposalToast(p));

registerCommandProvider(() => [
  {
    id: "auto.addGitReview",
    title: "Automation: Add git-review rule (watch folder → inject to label)",
    run: () => addGitReviewRule(),
  },
  {
    id: "auto.addTimer",
    title: "Automation: Add timer rule (inject every N minutes → label)",
    run: () => addTimerRule(),
  },
  { id: "auto.list", title: "Automation: List rules", run: () => manageRules() },
]);

registerCommandProvider(() => automationRuleCommands);
let automationRuleCommands: import("./commands").CommandItem[] = [];
async function refreshRuleCommands(): Promise<void> {
  const rules = await ipc.listRules().catch(() => []);
  automationRuleCommands = rules.flatMap((r) => [
    {
      id: `auto.run.${r.id}`,
      title: `Automation: Run rule now — ${r.name}`,
      run: async () => {
        try {
          const msg = await ipc.runRuleNow(r.id);
          showStatus(msg);
        } catch (e) {
          showStatus(String(e), true);
        }
      },
    },
    {
      id: `auto.toggle.${r.id}`,
      title: `Automation: ${r.enabled ? "Disable" : "Enable"} rule — ${r.name}`,
      run: async () => {
        await ipc.setRuleEnabled(r.id, !r.enabled).catch((e) => showStatus(String(e), true));
        await refreshRuleCommands();
      },
    },
    {
      id: `auto.remove.${r.id}`,
      title: `Automation: Remove rule — ${r.name}`,
      run: async () => {
        await ipc.removeRule(r.id).catch(() => {});
        await refreshRuleCommands();
        showStatus(`Removed rule "${r.name}"`);
      },
    },
  ]);
}

registerCommandProvider(() =>
  THEMES.map((t) => ({
    id: `theme.${t.id}`,
    title: `Theme: ${t.label}`,
    run: () => applyThemeById(t.id),
  })),
);

registerCommandProvider(() =>
  metas
    .filter((m) => m.id !== current?.id)
    .map((m) => ({
      id: `ws.switch.${m.id}`,
      title: `Workspace: Switch to "${m.name}"`,
      run: async () => {
        await switchTo(m.id);
      },
    })),
);

// ----------------------------------------------------- templates (Phase B)

async function applyTemplateWorkspace(
  template: import("./types").Template,
  params: Record<string, string>,
): Promise<void> {
  try {
    const snap = await ipc.applyTemplate(template, params);
    metas = snap.workspaces;
    // The new template workspace is the last one added; switch to it (which
    // spawns its sessions and runs any startupCommands).
    const created = metas[metas.length - 1];
    if (created) await switchTo(created.id);
    showStatus(`Applied template "${template.name}"`);
  } catch (e) {
    showStatus(String(e), true);
  }
}

/** Prompt for each declared template param, then apply. */
async function collectParamsAndApply(template: import("./types").Template): Promise<void> {
  const params: Record<string, string> = {};
  for (const p of template.params) {
    const res = await promptModal({
      title: p.prompt ?? `Value for \${${p.name}}`,
      initial: p.default ?? "",
      placeholder: p.name,
    });
    if (res === null) {
      restoreTermFocus();
      return; // cancelled
    }
    params[p.name] = res.value;
  }
  restoreTermFocus();
  await applyTemplateWorkspace(template, params);
}

async function applyNamedTemplate(name: string): Promise<void> {
  try {
    const template = await ipc.getTemplate(name);
    await collectParamsAndApply(template);
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function saveLayoutAsTemplate(): Promise<void> {
  if (!current) return;
  const res = await promptModal({
    title: "Save current layout as template — name",
    placeholder: "ai-pair-dev",
  });
  restoreTermFocus();
  if (res === null || !res.value.trim()) return;
  try {
    const template = await ipc.workspaceAsTemplate(current.id, res.value.trim());
    await ipc.saveTemplate(template);
    await refreshTemplateCommands();
    showStatus(
      `Saved template "${template.name}". Tip: edit its JSON to add \${params} and startupCommands.`,
    );
  } catch (e) {
    showStatus(String(e), true);
  }
}

async function applyRepoProfile(): Promise<void> {
  const leaf = activePaneId ? findLeaf(activePaneId) : undefined;
  const repoRes = await promptModal({
    title: "Apply .terminal-f/profile.json from which repo folder?",
    initial: leaf?.cwd ?? "",
    placeholder: "C:\\path\\to\\repo",
  });
  restoreTermFocus();
  if (repoRes === null || !repoRes.value.trim()) return;
  const repo = repoRes.value.trim();
  try {
    const profile = await ipc.readRepoProfile(repo);
    // Workspace trust: an untrusted repo profile that runs commands must be
    // explicitly confirmed before we materialize (and later auto-run) it.
    if (profile.hasCommands && !profile.trusted) {
      const confirm = await promptModal({
        title: `Trust ${repo}?`,
        initial: "",
        placeholder: "type: yes",
        checkboxLabel: "I trust this repo to run its startup commands",
        checkboxInitial: false,
        okLabel: "Trust & apply",
      });
      restoreTermFocus();
      if (confirm === null || !confirm.checked) {
        showStatus("Repo not trusted; profile not applied.", true);
        return;
      }
      await ipc.trustRepo(repo);
    }
    await collectParamsAndApply(profile.template);
  } catch (e) {
    showStatus(String(e), true);
  }
}

let templateCommands: import("./commands").CommandItem[] = [];
async function refreshTemplateCommands(): Promise<void> {
  const names = await ipc.listTemplates().catch(() => []);
  templateCommands = names.map((n) => ({
    id: `template.apply.${n}`,
    title: `Template: Apply "${n}"`,
    run: () => applyNamedTemplate(n),
  }));
}

registerCommandProvider(() => [
  { id: "template.saveLayout", title: "Template: Save current layout as template", run: () => saveLayoutAsTemplate() },
  { id: "template.repoProfile", title: "Template: Apply repo profile (.terminal-f/profile.json)", run: () => applyRepoProfile() },
]);
registerCommandProvider(() => templateCommands);

// ------------------------------------------------------------------ events

void ipc.onPtyOutput((ev) => {
  // Output path: backend ring buffer -> batched event -> xterm.write.
  const view = terms.getView(ev.paneId);
  if (!view) return; // pane not mounted; ring buffer replay covers it later
  view.term.write(ev.data);
  view.lastSeq = ev.seq;
});

void ipc.onPtyExit((ev) => {
  const info = sessionInfoByPane.get(ev.paneId);
  if (info) {
    sessionInfoByPane.set(ev.paneId, { ...info, state: "exited", exitCode: ev.exitCode });
  }
  updateHeaders();
  const view = terms.getView(ev.paneId);
  if (view && !view.exitShown) {
    view.exitShown = true;
    view.term.write(`\r\n\x1b[31m[process exited (code ${ev.exitCode ?? "?"})]\x1b[0m\r\n`);
  }
});

// --------------------------------------------------------------- activity

async function pollActivity(): Promise<void> {
  try {
    const list = await ipc.workspaceActivity();
    const next = new Map(list.map((a) => [a.workspaceId, a]));
    const changed = JSON.stringify([...next.entries()]) !== JSON.stringify([...activity.entries()]);
    activity = next;
    if (changed) refreshSidebar();
  } catch {
    /* backend busy or shutting down; try next tick */
  }
}

// -------------------------------------------------------------------- boot

async function boot(): Promise<void> {
  const bootInfo = await ipc.getBootInfo().catch(() => ({ autotest: false, shell: null }));
  if (bootInfo.shell) defaultShellName = programName(bootInfo.shell);
  const snap = await ipc.getState();
  metas = snap.workspaces;
  uiPrefs = {
    theme: typeof snap.ui?.theme === "string" ? snap.ui.theme : undefined,
    fontSize: typeof snap.ui?.fontSize === "number" ? snap.ui.fontSize : undefined,
    sidebar: typeof snap.ui?.sidebar === "object" && snap.ui.sidebar ? snap.ui.sidebar : {},
    copyOnSelect: snap.ui?.copyOnSelect === true,
  };
  setTheme(themeById(uiPrefs.theme));
  if (uiPrefs.fontSize) terms.applyTerminalOptions({ fontSize: uiPrefs.fontSize });
  terms.setCopyOnSelect(uiPrefs.copyOnSelect === true);
  refreshSidebar();
  const target = snap.activeWorkspaceId ?? metas[0]?.id;
  if (target) await switchTo(target);
  window.setInterval(() => void pollActivity(), 1000);
  void refreshRuleCommands();
  void refreshTemplateCommands();

  // File drag-drop (ADR-010): Tauri intercepts OS file drops (HTML5 drop
  // never fires), so consume its drag-drop event instead. Dropped paths are
  // pasted into the pane under the cursor — standard terminal drop behavior.
  void getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type !== "drop" || p.paths.length === 0) return;
    const scale = window.devicePixelRatio || 1;
    const at = document.elementFromPoint(p.position.x / scale, p.position.y / scale);
    const host = at?.closest?.(".term-host") as HTMLElement | null;
    const paneId = (host?.dataset.paneId as PaneId | undefined) ?? activePaneId;
    if (!paneId) return;
    focusPane(paneId);
    terms.pastePathsToPane(paneId, p.paths);
  });

  if (bootInfo.autotest) {
    const ctl: AutotestCtl = {
      currentWorkspaceId: () => current?.id ?? "",
      activePaneId: () => activePaneId ?? "",
      paneIds: () => (current ? collectLeaves(current.root).map((l) => l.id) : []),
      switchTo,
      splitActive,
      addWorkspace,
      readBuffer: terms.readBufferText,
      lastSwitchMs: () => lastSwitchMs,
    };
    // Give shells a moment to initialize before scripting them.
    setTimeout(() => void runAutotest(ctl), 1500);
  }
}

void boot().catch((e) => {
  showStatus(`boot failed: ${String(e)}`, true);
  console.error(e);
});
