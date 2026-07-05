// xterm.js instance management (spec 9).
//
// xterm owns: screen rendering, keyboard capture, visual scrollback, and
// visual snapshot/restore (serialize addon). It does NOT own PTY processes
// or authoritative output capture — those live in the Rust backend.

import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { WebglAddon } from "@xterm/addon-webgl";
import { WebLinksAddon } from "@xterm/addon-web-links";
import * as ipc from "./ipc";
import { currentTheme } from "./themes";
import type { PaneId } from "./types";

let fontSize = 14;

// Copy-on-select: when on, completing a selection copies it to the clipboard
// automatically (opt-in, persisted in uiPrefs; default off). Mirrors the
// classic X11/terminal behaviour some users expect.
let copyOnSelect = false;

// Ctrl/Cmd+click on a linkified URL in terminal output opens it in the browser.
// The web-links addon underlines URLs on hover; the modifier gate keeps plain
// click free for text selection. On (persisted in uiPrefs; default on).
let openUrlOnClick = true;

export function currentFontSize(): number {
  return fontSize;
}

export function setCopyOnSelect(on: boolean): void {
  copyOnSelect = on;
}

export function setOpenUrlOnClick(on: boolean): void {
  openUrlOnClick = on;
}

/** Whether a link-activation mouse event should open the URL: the feature must
 *  be enabled and the click must carry Ctrl (or Cmd on macOS). Pure — the
 *  web-links handler and the autotest both use this. */
export function shouldActivateLink(
  ev: { ctrlKey?: boolean; metaKey?: boolean },
  enabled: boolean,
): boolean {
  return enabled && !!(ev.ctrlKey || ev.metaKey);
}

/** Decide what a keydown means for multiline input (single source of truth for
 * the key handler and its tests). Ctrl+Enter / Shift+Enter insert a newline;
 * while an IME syllable is composing we defer to compositionend so the
 * character commits first. Returns null for keys we don't claim. */
export type NewlineChord = "send" | "defer" | null;
export function newlineChordFor(ev: {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  isComposing?: boolean;
  keyCode?: number;
}): NewlineChord {
  if (ev.key !== "Enter" || ev.altKey || ev.metaKey) return null;
  if (!ev.ctrlKey && !ev.shiftKey) return null;
  return ev.isComposing || ev.keyCode === 229 ? "defer" : "send";
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

/** Copy the pane's current selection to the OS clipboard via the backend
 * (arboard), matching the paste bridge's approach. `clear` deselects after
 * copying — true for explicit copy (so a second Ctrl+C sends ^C, like Windows
 * Terminal), false for copy-on-select (deselecting mid-drag would be jarring).
 * Returns false when there is nothing selected. */
export async function copySelection(paneId: PaneId, clear = true): Promise<boolean> {
  const view = views.get(paneId);
  if (!view) return false;
  const text = view.term.getSelection();
  if (!text) return false;
  try {
    await ipc.copyToClipboard(text);
    if (clear) view.term.clearSelection();
    return true;
  } catch (e) {
    console.warn("[copy]", e);
    return false;
  }
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

/** Decode a base64 payload as UTF-8 text (OSC 52 clipboard payloads are UTF-8
 * base64, so a naive atob would mangle non-ASCII). Returns null on bad input. */
function decodeBase64Utf8(b64: string): string | null {
  try {
    const bin = atob(b64);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    return new TextDecoder().decode(bytes);
  } catch {
    return null;
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
  // Linkify http(s) URLs in output; Ctrl/Cmd+click opens them via the validated
  // backend path (replaces the addon's default window.open). Auto-disposed by
  // term.dispose(). Plain click passes through so text selection still works.
  term.loadAddon(
    new WebLinksAddon((event, uri) => {
      if (!shouldActivateLink(event, openUrlOnClick)) return;
      void ipc.openExternalUrl(uri).catch((e) => console.warn("[openUrl]", e));
    }),
  );

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

  // Multiline newline chord state (see the key handler below). A newline chord
  // pressed during — or immediately after — an IME composition must not send
  // the newline until the committed syllable has been written, or the newline
  // races ahead and the last character lands on the next line. So we set
  // `pendingNewline` and let onData attach the newline to the committed text.
  //
  // `composing` / `awaitingComposedData` track the composition window ourselves
  // because in Chromium the Enter keydown often arrives *after* compositionend
  // (isComposing already false) while xterm still delivers the committed text
  // on a later tick — the exact case a naive isComposing check misses.
  let pendingNewline = false;
  let composing = false;
  let awaitingComposedData = false;

  // Input path: xterm onData -> writePane(paneId, data) -> backend PTY writer.
  term.onData((data) => {
    awaitingComposedData = false; // this chunk is (or follows) the committed text
    // A newline chord is pending from a composition: append it after the text,
    // atomically, so ordering is guaranteed without racing timers.
    const out = pendingNewline ? data + "\x1b\r" : data;
    pendingNewline = false;
    ipc.writePane(paneId, out).catch((e) => console.warn("[writePane]", e));
  });
  // Keep app-level shortcuts (split/close) out of the PTY input stream.
  // Copy/paste are claimed like Windows Terminal (xterm would otherwise send
  // ^C/^V to the PTY and cancel the browser events), and read/written through
  // the backend (arboard) to avoid WebView clipboard permission prompts:
  //   Ctrl+Shift+C        -> always copy the selection
  //   Ctrl+C w/ selection -> copy + deselect (a 2nd press then sends ^C/SIGINT)
  //   Ctrl+C w/o selection -> pass ^C through untouched
  //   Ctrl+V              -> paste OS clipboard (text, or a saved image's path)
  //   Ctrl+Enter / Shift+Enter -> insert a newline (multiline input, see below)
  //
  // Multiline input: Claude Code (and VS Code's terminal-setup convention)
  // read ESC+CR (\x1b\r, "Meta+Enter") as "insert a newline" instead of submit.
  // xterm always sends a bare \r for Enter regardless of modifiers, so we
  // translate the chord ourselves.
  //
  // Korean/IME safety (the hard part): the newline chord must never disturb an
  // in-progress or just-finished composition. Three cases:
  //   - composing now (isComposing / keyCode 229 / our flag): let the IME
  //     commit (return true, no preventDefault); onData appends the newline.
  //   - composition just ended (awaitingComposedData): the committed text is
  //     still in flight; suppress the Enter and let onData append the newline.
  //   - no composition: send the newline immediately.
  const sendNewline = () =>
    ipc.writePane(paneId, "\x1b\r").catch((e) => console.warn("[multiline]", e));

  term.attachCustomKeyEventHandler((ev) => {
    if (opts.isShortcut(ev)) return false;
    if (ev.type !== "keydown") return true;

    const chord = newlineChordFor(ev);
    if (chord !== null) {
      const composingNow = chord === "defer" || composing;
      if (composingNow) {
        pendingNewline = true; // onData appends after the IME commits
        return true; // let the composition commit; don't suppress the key
      }
      if (awaitingComposedData) {
        ev.preventDefault(); // suppress the bare \r so it doesn't submit
        pendingNewline = true; // onData appends after the in-flight text
        return false;
      }
      ev.preventDefault();
      void sendNewline();
      return false;
    }

    // Never disturb an in-progress IME composition with the shortcuts below.
    if (ev.isComposing || ev.keyCode === 229 || composing) return true;

    if (ev.ctrlKey && !ev.altKey && !ev.metaKey) {
      const k = ev.key.toLowerCase();
      if (k === "c" && ev.shiftKey) {
        ev.preventDefault();
        void copySelection(paneId);
        return false;
      }
      if (k === "c" && !ev.shiftKey && term.hasSelection()) {
        ev.preventDefault();
        void copySelection(paneId);
        return false;
      }
      if (k === "v" && !ev.shiftKey) {
        ev.preventDefault(); // suppress any native paste path too
        void pasteViaBackend(paneId);
        return false;
      }
    }
    return true;
  });

  // Track the composition window ourselves. `awaitingComposedData` stays true
  // from compositionend until the committed text lands in onData, so a chord
  // keydown arriving in that gap (isComposing already false) still defers.
  term.textarea?.addEventListener("compositionstart", () => {
    composing = true;
    awaitingComposedData = false;
  });
  term.textarea?.addEventListener("compositionend", () => {
    composing = false;
    awaitingComposedData = true;
    // Fallback: onData normally clears these within a tick. If the composition
    // delivered no text (aborted), flush any pending newline and clear state so
    // it can't attach to unrelated later input. 120ms is well past onData's
    // same-tick delivery, so this never races ahead of committed text.
    setTimeout(() => {
      awaitingComposedData = false;
      if (pendingNewline) {
        pendingNewline = false;
        void sendNewline();
      }
    }, 120);
  });

  // Copy-on-select (opt-in): copy without deselecting so the highlight stays.
  term.onSelectionChange(() => {
    if (copyOnSelect && term.hasSelection()) void copySelection(paneId, false);
  });

  // Right-click: copy when there's a selection, else paste — the Windows
  // Terminal convention. Suppress the native context menu either way.
  el.addEventListener("contextmenu", (ev) => {
    ev.preventDefault();
    if (term.hasSelection()) void copySelection(paneId);
    else void pasteViaBackend(paneId);
  });

  // OSC 52 clipboard write: TUIs (Claude Code, tmux, vim, neovim) copy to the
  // system clipboard by emitting ESC ] 52 ; <sel> ; <base64> ST. xterm.js has
  // no clipboard binding and drops OSC 52 by default, so those copies silently
  // vanished — the reason "copy inside Claude Code" pasted stale content. We
  // decode the payload and write it to the OS clipboard via the backend.
  // Read requests (payload "?") are refused: honoring them would let any
  // process running in a pane exfiltrate the user's clipboard.
  term.parser.registerOscHandler(52, (data) => {
    const semi = data.indexOf(";");
    const payload = semi >= 0 ? data.slice(semi + 1) : data;
    if (payload === "" || payload === "?") return true; // reject reads; nothing to write
    if (payload.length > 8 * 1024 * 1024) return true; // ignore absurd payloads
    const text = decodeBase64Utf8(payload);
    if (text) void ipc.copyToClipboard(text).catch((e) => console.warn("[osc52]", e));
    return true; // handled: don't let xterm print the raw sequence
  });

  // OSC 9;9 (ConEmu/WT cwd report) is consumed by the backend reader for live
  // cwd tracking (ADR-011); swallow it here so it never renders as stray text.
  // Other OSC 9 uses (9;4 progress, notifications) pass through untouched.
  term.parser.registerOscHandler(9, (data) => data.startsWith("9;"));

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
