// xterm.js instance management (spec 9).
//
// xterm owns: screen rendering, keyboard capture, visual scrollback, and
// visual snapshot/restore (serialize addon). It does NOT own PTY processes
// or authoritative output capture — those live in the Rust backend.

import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { WebglAddon } from "@xterm/addon-webgl";
import * as ipc from "./ipc";
import { currentTheme } from "./themes";
import type { PaneId } from "./types";

let fontSize = 14;

export function currentFontSize(): number {
  return fontSize;
}

/** Apply theme/font to all live terminals (xterm supports live updates). */
export function applyTerminalOptions(opts: { fontSize?: number }): void {
  if (opts.fontSize) fontSize = Math.min(28, Math.max(9, opts.fontSize));
  for (const view of views.values()) {
    view.term.options.theme = currentTheme().term;
    view.term.options.fontSize = fontSize;
    syncSize(view);
  }
}

export interface PaneView {
  paneId: PaneId;
  term: Terminal;
  fit: FitAddon;
  serialize: SerializeAddon;
  /** Outer host element (header + terminal body); reparented by the renderer. */
  el: HTMLElement;
  headerIndex: HTMLElement;
  headerTitle: HTMLElement;
  headerLabels: HTMLElement;
  headerInject: HTMLElement;
  headerObserve: HTMLElement;
  lastSeq: number;
  resizeObserver: ResizeObserver;
  exitShown: boolean;
}

export interface VisualSnapshot {
  data: string;
  lastSeq: number;
}

const views = new Map<PaneId, PaneView>();
// Visual snapshots of unmounted (inactive-workspace) panes (ADR-002/003).
export const snapshots = new Map<PaneId, VisualSnapshot>();

export function getView(paneId: PaneId): PaneView | undefined {
  return views.get(paneId);
}

export function allViews(): PaneView[] {
  return [...views.values()];
}

interface CreateOpts {
  onFocusRequest: (paneId: PaneId) => void;
  onCloseRequest: (paneId: PaneId) => void;
  onZoomRequest: (paneId: PaneId) => void;
  isShortcut: (ev: KeyboardEvent) => boolean;
}

function quotePath(p: string): string {
  return /\s/.test(p) ? `"${p}"` : p;
}

/** Ctrl+V bridge (ADR-010): read the OS clipboard via the backend and paste
 * text as text, or a clipboard image as a saved file's path. Called from the
 * key handler because xterm cancels the keydown, so the browser never fires a
 * paste event for Ctrl+V. */
export async function pasteViaBackend(paneId: PaneId): Promise<string> {
  const view = views.get(paneId);
  if (!view) return "none";
  try {
    const res = await ipc.pasteClipboard();
    if (res.kind === "text") view.term.paste(res.data);
    else if (res.kind === "imagePath") view.term.paste(`${quotePath(res.data)} `);
    return res.kind;
  } catch (e) {
    console.warn("[paste]", e);
    return "error";
  }
}

/** Drag-drop bridge: paste dropped file paths (from Tauri's drag-drop event,
 * which suppresses HTML5 drop) into a pane, quoted, space-separated. */
export function pastePathsToPane(paneId: PaneId, paths: string[]): boolean {
  const view = views.get(paneId);
  if (!view || paths.length === 0) return false;
  view.term.paste(paths.map(quotePath).join(" ") + " ");
  return true;
}

/** Save a pasted image blob to disk and paste its path into the pane. The
 * path goes through term.paste(), so bracketed-paste wrapping matches what a
 * file drag-drop produces (which is how Claude Code detects image paths). */
export async function pasteImageBlob(paneId: PaneId, blob: Blob): Promise<boolean> {
  const view = views.get(paneId);
  if (!view) return false;
  try {
    const b64 = await blobToBase64(blob);
    const path = await ipc.savePastedImage(b64, blob.type || "image/png");
    view.term.paste(`${quotePath(path)} `);
    return true;
  } catch (e) {
    console.warn("[pasteImage]", e);
    return false;
  }
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).split(",", 2)[1] ?? "");
    r.onerror = () => reject(r.error ?? new Error("blob read failed"));
    r.readAsDataURL(blob);
  });
}

export function getOrCreateView(paneId: PaneId, opts: CreateOpts): PaneView {
  const existing = views.get(paneId);
  if (existing) return existing;

  const el = document.createElement("div");
  el.className = "term-host";
  el.dataset.paneId = paneId;

  // Slim identification tab. Hidden via CSS when the workspace has a single
  // pane (#panes.single-pane) so it costs no space where it isn't needed.
  const header = document.createElement("div");
  header.className = "pane-header";
  const headerIndex = document.createElement("span");
  headerIndex.className = "pane-index";
  const headerTitle = document.createElement("span");
  headerTitle.className = "pane-title";
  const headerLabels = document.createElement("span");
  headerLabels.className = "pane-labels";
  const headerInject = document.createElement("span");
  headerInject.className = "pane-inject";
  headerInject.textContent = "⚡";
  headerInject.title = "Injection allowed on this pane (M2.0 allowlist)";
  headerInject.style.display = "none";
  const headerObserve = document.createElement("span");
  headerObserve.className = "pane-observe";
  headerObserve.textContent = "👁";
  headerObserve.title = "Output observation allowed (M2.2 control API)";
  headerObserve.style.display = "none";
  const headerClose = document.createElement("button");
  headerClose.className = "pane-close";
  headerClose.textContent = "×";
  headerClose.title = "Close pane (Ctrl+Shift+W)";
  header.append(headerIndex, headerTitle, headerLabels, headerInject, headerObserve, headerClose);
  header.addEventListener("mousedown", () => opts.onFocusRequest(paneId));
  header.addEventListener("dblclick", () => opts.onZoomRequest(paneId));
  headerClose.addEventListener("click", (e) => {
    e.stopPropagation();
    opts.onCloseRequest(paneId);
  });

  const body = document.createElement("div");
  body.className = "term-body";
  el.append(header, body);

  const term = new Terminal({
    allowProposedApi: true,
    scrollback: 5000,
    fontFamily: '"Cascadia Mono", Consolas, "Courier New", monospace',
    fontSize,
    theme: currentTheme().term,
  });
  const fit = new FitAddon();
  const serialize = new SerializeAddon();
  term.loadAddon(fit);
  term.loadAddon(serialize);

  const view: PaneView = {
    paneId,
    term,
    fit,
    serialize,
    el,
    headerIndex,
    headerTitle,
    headerLabels,
    headerInject,
    headerObserve,
    lastSeq: 0,
    resizeObserver: new ResizeObserver(() => syncSize(view)),
    exitShown: false,
  };

  term.open(body);
  tryWebgl(term);

  // Input path: xterm onData -> writePane(paneId, data) -> backend PTY writer.
  term.onData((data) => {
    ipc.writePane(paneId, data).catch((e) => console.warn("[writePane]", e));
  });
  // Keep app-level shortcuts (split/close) out of the PTY input stream.
  // Ctrl+V is claimed as paste (like Windows Terminal): xterm would otherwise
  // send ^V to the PTY and cancel the event, so no browser paste ever fires.
  // We read the OS clipboard via the backend instead (text or image).
  term.attachCustomKeyEventHandler((ev) => {
    if (opts.isShortcut(ev)) return false;
    if (
      ev.type === "keydown" &&
      ev.ctrlKey &&
      !ev.altKey &&
      !ev.metaKey &&
      ev.key.toLowerCase() === "v"
    ) {
      ev.preventDefault(); // suppress any native paste path too
      void pasteViaBackend(paneId);
      return false;
    }
    return true;
  });

  // Image paste bridge (ADR-010). Windows terminals forward only clipboard
  // TEXT on Ctrl+V, so image-aware TUIs (Claude Code) never see screenshots.
  // When the clipboard holds an image and no text, save it to a temp file and
  // paste the file path (bracketed) — the drag-drop shape such TUIs handle.
  // Capture phase so this runs before xterm's own textarea paste handler.
  el.addEventListener(
    "paste",
    (ev) => {
      const cd = ev.clipboardData;
      if (!cd || cd.getData("text/plain")) return; // text wins: default paste
      const item = [...cd.items].find((i) => i.type.startsWith("image/"));
      const file = item?.getAsFile();
      if (!file) return;
      ev.preventDefault();
      ev.stopPropagation();
      void pasteImageBlob(paneId, file);
    },
    true,
  );

  el.addEventListener("mousedown", () => opts.onFocusRequest(paneId));
  term.textarea?.addEventListener("focus", () => opts.onFocusRequest(paneId));

  view.resizeObserver.observe(el);
  views.set(paneId, view);
  return view;
}

function tryWebgl(term: Terminal): void {
  // WebGL renderer with graceful fallback (spec 1): on load failure or
  // context loss we dispose the addon and xterm reverts to the DOM renderer.
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => {
      console.warn("[terminal-f] WebGL context lost; falling back to DOM renderer");
      webgl.dispose();
    });
    term.loadAddon(webgl);
  } catch (e) {
    console.warn("[terminal-f] WebGL unavailable; using DOM renderer", e);
  }
}

export function setPaneHeader(
  paneId: PaneId,
  index: number,
  title: string,
  exited: boolean,
  labels: string[] = [],
  allowInjection = false,
  allowObserve = false,
): void {
  const view = views.get(paneId);
  if (!view) return;
  view.headerIndex.textContent = String(index);
  view.headerTitle.textContent = title;
  view.headerTitle.classList.toggle("exited", exited);
  view.headerLabels.replaceChildren(
    ...labels.map((l) => {
      const chip = document.createElement("span");
      chip.className = "pane-label-chip";
      chip.textContent = l;
      return chip;
    }),
  );
  view.headerInject.style.display = allowInjection ? "" : "none";
  view.headerObserve.style.display = allowObserve ? "" : "none";
}

export function syncSize(view: PaneView): void {
  if (!view.el.isConnected || view.el.clientWidth < 20 || view.el.clientHeight < 20) return;
  try {
    view.fit.fit();
    const { rows, cols } = view.term;
    ipc.resizePty(view.paneId, rows, cols).catch(() => {
      /* session may not exist yet; backend replays correct size on spawn */
    });
  } catch {
    /* ignore transient fit errors during layout */
  }
}

/** Serialize the visual state and fully dispose the xterm instance
 * (unmount policy for inactive workspaces, ADR-002). */
export function snapshotAndDispose(paneId: PaneId): void {
  const view = views.get(paneId);
  if (!view) return;
  try {
    snapshots.set(paneId, {
      data: view.serialize.serialize({ scrollback: 1000 }),
      lastSeq: view.lastSeq,
    });
  } catch (e) {
    console.warn("[terminal-f] serialize failed", e);
    snapshots.set(paneId, { data: "", lastSeq: view.lastSeq });
  }
  disposeView(paneId);
}

export function disposeView(paneId: PaneId): void {
  const view = views.get(paneId);
  if (!view) return;
  view.resizeObserver.disconnect();
  view.term.dispose();
  view.el.remove();
  views.delete(paneId);
}

export function dropPaneState(paneId: PaneId): void {
  disposeView(paneId);
  snapshots.delete(paneId);
}

/** Read the visible buffer as plain text (used by autotest checks). */
export function readBufferText(paneId: PaneId): string {
  const view = views.get(paneId);
  if (!view) return "";
  const buf = view.term.buffer.active;
  const lines: string[] = [];
  for (let i = 0; i < buf.length; i++) {
    lines.push(buf.getLine(i)?.translateToString(true) ?? "");
  }
  return lines.join("\n");
}
