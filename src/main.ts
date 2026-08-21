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
import { promptModal } from "./modal";
import {
  collectLeaves,
  firstPaneId,
  type Direction,
  type PaneId,
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

function toggleOpenUrlOnClick(): void {
  const next = !(uiPrefs.openUrlOnClick !== false); // default on -> first toggle turns off
  uiPrefs.openUrlOnClick = next;
  terms.setOpenUrlOnClick(next);
  saveUiPrefs();
  showStatus(`Ctrl+click to open URLs ${next ? "enabled" : "disabled"}`);
}

// First-launch default-on (v0.1.x): install both pwsh $PROFILE blocks once,
// without the interactive confirm. Bump the stamp when a snippet change
// should re-run the one-shot install — the install is idempotent and
// refreshes an older block in place.
const SHELL_INTG_AUTO_VER = "v1";

async function autoInstallShellIntegration(): Promise<void> {
  for (const feature of ["multiline", "cwd"] as const) {
    try {
      await ipc.installPwshIntegration(feature);
    } catch (e) {
      // Leave the stamp unset so the next launch retries. A user who removed
      // the blocks manually is past this point (the stamp was already set).
      console.warn("[autoInstallShellIntegration]", feature, e);
      return;
    }
  }
  uiPrefs.pwshIntegrationAuto = SHELL_INTG_AUTO_VER;
  saveUiPrefs();
  showStatus("PowerShell multiline + directory tracking enabled. Open a NEW PowerShell pane to use it.");
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
    onPickRoot: (id) => void pickWorkspaceRoot(id),
    onClearRoot: (id) => void applyWorkspaceRoot(id, null),
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
  // SPEC-PTY-FLOW-001 M2 (R10): replayInFlight 를 가장 먼저 세워 live pty-output 이벤트를
  // 버퍼링한다 — 스냅샷 복원·replay 데이터 적용 "이후에" 순서대로 붙인다(R10 N2).
  view.replayInFlight = true;
  // 5. restore visual snapshot (정본 parsedSeq 기준)
  const snap = terms.snapshots.get(paneId);
  if (snap) {
    view.parsedSeq = snap.lastSeq; // 스냅샷은 parsedSeq 까지 파싱된 상태
    if (snap.data) terms.writeParsedNoAck(paneId, snap.data); // 시각 복원 — ack 없음
  } else {
    view.parsedSeq = 0;
  }
  terms.snapshots.delete(paneId);
  try {
    // 6. replay pending backend output accumulated while unmounted
    const replay = await ipc.replayPane(paneId, view.parsedSeq);
    if (replay.dropped) {
      terms.writeParsedNoAck(
        paneId,
        "\r\n\x1b[33m[terminal-f: output overflow while inactive, oldest chunks dropped]\x1b[0m\r\n",
      );
    }
    if (replay.data) {
      // R13: replay 바이트는 ack 않함(byte 회계는 live-emit 전용). parsedSeq는 파싱
      // 완료 시 전진 → writeParsedNoAck 콜백이 replay.lastSeq 로 갱신. await 로 cb 확정.
      await terms.writeParsedNoAck(paneId, replay.data, replay.lastSeq);
    } else if (replay.lastSeq > view.parsedSeq) {
      // replay 데이터는 없으나 백엔드 lastSeq 가 앞서면 동기화(공백 없음 보증).
      view.parsedSeq = replay.lastSeq;
    }
    if (replay.state === "exited" && !view.exitShown) {
      view.exitShown = true;
      terms.writeParsedNoAck(paneId, "\r\n\x1b[31m[process exited]\x1b[0m\r\n");
    }
  } catch (e) {
    // Pane without a session (e.g. PTY cap reached): show why.
    terms.writeParsedNoAck(paneId, `\r\n\x1b[31m[no session: ${String(e)}]\x1b[0m\r\n`);
  } finally {
    view.replayInFlight = false;
    // replay 중 버퍼링된 live 이벤트를 순서대로 적용. replay 가 이미 포함한 범위
    // (ev.seq <= parsedSeq)는 중복이므로 스킵(R10 N2 — 중복 재방출 방지).
    const pending = view.pendingReplayEvents.splice(0);
    for (const ev of pending) {
      if (ev.seq <= view.parsedSeq) continue;
      view.receivedSeq = ev.seq;
      terms.writeOutput(paneId, ev.data, { seq: ev.seq, byteLen: ev.byteLen });
    }
  }
}

function noteExitedSessions(sessions: SessionInfo[]): void {
  for (const s of sessions) {
    sessionInfoByPane.set(s.paneId, s);
    const view = terms.getView(s.paneId);
    if (view && s.state === "exited" && !view.exitShown) {
      view.exitShown = true;
      terms.writeOutput(s.paneId, `\r\n\x1b[31m[process exited (code ${s.exitCode ?? "?"})]\x1b[0m\r\n`);
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
    // SPEC-PTY-FLOW-001 M2: snapshotAndDispose is async (R11 드레인 + R15 late-ack
    // 플러시). 모든 leaf 의 dispose-직전 ack invoke 가 정착한 뒤 switchWorkspace 가
    // 백엔드 회계를 리셋하도록 병렬 await 한다(도착 순서 "마지막 ack → 리셋").
    await Promise.all(
      collectLeaves(current.root).map((leaf) => terms.snapshotAndDispose(leaf.id)),
    );
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

// ------------------------------------------------------ workspace root (M4)

// metas에서 현재 rootDir을 읽어 폴더 대화상자를 열고, 선택하면 적용한다.
// 취소(pick_folder가 null 반환) 시 조용히 조기 반환한다 — IPC·토스트·재렌더 없음 (R10).
async function pickWorkspaceRoot(id: string): Promise<void> {
  const initial = metas.find((m) => m.id === id)?.rootDir ?? undefined;
  let dir: string | null;
  try {
    dir = await ipc.pickFolder(initial);
  } catch (e) {
    showStatus(String(e), true);
    return;
  }
  if (dir === null) return; // 취소 — 조용히 반환 (R10)
  await applyWorkspaceRoot(id, dir);
}

// set_workspace_root 결과로 metas/current를 갱신하고 사이드바를 다시 그린다.
// 성공 토스트는 "재시작 후 적용"을 명시한다 — 살아 있는 터미널이 그대로인 것이
// 버그가 아니라 설명된 동작으로 읽히도록 한다.
async function applyWorkspaceRoot(id: string, dir: string | null): Promise<void> {
  try {
    const res = await ipc.setWorkspaceRoot(id, dir);
    metas = res.workspaces;
    if (current?.id === id) current = res.workspace;
    refreshSidebar();
    showStatus(
      dir
        ? "시작 폴더를 설정했습니다 — 재시작 후 적용됩니다."
        : "시작 폴더를 해제했습니다 — 재시작 후 적용됩니다.",
    );
  } catch (e) {
    showStatus(String(e), true); // "not an existing folder" 등 백엔드 오류 노출
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
  {
    id: "ws.root",
    title: "Workspace: Set root folder…",
    run: () => {
      if (current) void pickWorkspaceRoot(current.id);
    },
  },
  {
    id: "ws.root.clear",
    title: "Workspace: Clear root folder",
    run: () => {
      if (current) void applyWorkspaceRoot(current.id, null);
    },
  },
  { id: "sidebar.toggle", title: "View: Toggle sidebar", hint: "Ctrl+Shift+B", run: () => toggleSidebar() },
  { id: "font.bigger", title: "View: Increase font size", run: () => setFontSize(terms.currentFontSize() + 1) },
  { id: "font.smaller", title: "View: Decrease font size", run: () => setFontSize(terms.currentFontSize() - 1) },
  {
    id: "copy.onSelect",
    title: `Copy: ${uiPrefs.copyOnSelect ? "Disable" : "Enable"} copy-on-select`,
    run: () => toggleCopyOnSelect(),
  },
  {
    id: "links.toggleOpen",
    title: `Links: ${uiPrefs.openUrlOnClick !== false ? "Disable" : "Enable"} Ctrl+click to open URLs`,
    run: () => toggleOpenUrlOnClick(),
  },
]);

// --------------------------------------------------------- pane helpers

function findLeaf(paneId: string) {
  return current ? collectLeaves(current.root).find((l) => l.id === paneId) : undefined;
}

function applyWorkspaceResult(res: { workspace: Workspace }): void {
  current = res.workspace;
  updateHeaders();
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

registerCommandProvider(() => [
  { id: "inject.labels", title: "Pane: Edit labels (focused)", run: () => editPaneLabels() },
]);

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
  if (!view) return; // disposed/unmounted pane — discard (R10): 파싱도 ack 도 않함
  view.receivedSeq = ev.seq; // 진단용 — 정본 parsedSeq 는 write 콜백에서만 전진(R10)
  if (view.replayInFlight) {
    view.pendingReplayEvents.push(ev); // replay 진행중 — 버퍼링, 이후 순서 적용(R10 N2)
    return;
  }
  // SPEC-PTY-FLOW-002 R2: ack 수치는 이벤트 byteLen(백엔드 단일 원천)으로.
  terms.writeOutput(ev.paneId, ev.data, { seq: ev.seq, byteLen: ev.byteLen }); // ack 는 write 콜백에서(R9/R12)
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
    terms.writeOutput(ev.paneId, `\r\n\x1b[31m[process exited (code ${ev.exitCode ?? "?"})]\x1b[0m\r\n`);
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
    copyOnSelect: snap.ui?.copyOnSelect !== false, // default on when unset
    openUrlOnClick: snap.ui?.openUrlOnClick !== false, // default on when unset
    pwshIntegrationAuto:
      typeof snap.ui?.pwshIntegrationAuto === "string" ? snap.ui.pwshIntegrationAuto : undefined,
  };
  setTheme(themeById(uiPrefs.theme));
  if (uiPrefs.fontSize) terms.applyTerminalOptions({ fontSize: uiPrefs.fontSize });
  terms.setCopyOnSelect(uiPrefs.copyOnSelect !== false);
  terms.setOpenUrlOnClick(uiPrefs.openUrlOnClick !== false);
  refreshSidebar();
  const target = snap.activeWorkspaceId ?? metas[0]?.id;
  if (target) await switchTo(target);
  window.setInterval(() => void pollActivity(), 1000);
  void refreshTemplateCommands();

  // @MX:ANCHOR: [AUTO] 첫 실행 자동 설치 트리거 — 스탬프 불일치 시 1회 실행 계약(R2/R4/R8)
  // @MX:REASON: autotest 게이트·실패 재시도·버전 상향 재실행이 이 조건문 하나에 걸려 있다
  // @MX:SPEC: SPEC-DEFAULTON-001
  // Default-on shell integration for fresh installs: one-shot after boot so
  // the pwsh cold start never blocks first paint. Never under autotest — test
  // runs must not edit the dev machine's profile.
  if (!bootInfo.autotest && uiPrefs.pwshIntegrationAuto !== SHELL_INTG_AUTO_VER) {
    void autoInstallShellIntegration();
  }

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
