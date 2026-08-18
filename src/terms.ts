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
import type { PaneId, PtyOutputEvent } from "./types";

let fontSize = 14;

// IME output-buffering guards (see writeOutput / the composition listeners).
// While a Korean/IME syllable is composing we hold PTY output in a per-view
// buffer instead of writing it, because every xterm render calls
// CompositionHelper.updateCompositionElements() and repositions the hidden IME
// textarea to the cursor. A pane streaming output (e.g. Claude Code printing
// dozens of times a second) would yank the textarea mid-composition and corrupt
// the syllable, since the final commit is textarea.value.substring-based and so
// sensitive to that geometry churn. We flush on compositionend. Two safety
// valves keep a stuck-open composition from freezing output forever:
//   - watchdog: force-flush if output has been held this long, and give up
//     buffering until the next compositionstart.
//   - size cap: force-flush if the held buffer grows past this (runaway
//     process memory guard). Buffering stays armed after a size-cap flush.
//   - echo pass-through: output arriving within this window of the user's own
//     input (onData -> PTY) is the echo/redraw of a just-committed syllable and
//     must render immediately, even mid-composition — otherwise, in continuous
//     Korean typing, the next syllable's compositionstart traps the previous
//     syllable's echo, the cursor never advances, and the IME preview stacks
//     over the uncommitted text at one cell. A single echo render is safe for
//     the IME (idle-shell Korean typing always worked); it's the sustained
//     streaming between commits that corrupts composition.
const OUTPUT_WATCHDOG_MS = 1000;
const OUTPUT_BUFFER_CAP = 256 * 1024;
const ECHO_PASS_MS = 120;
// @MX:NOTE: [AUTO] SPEC-PTY-FLOW-001 흐름제어 상수 (spec §B 명명 상수 — 매직 넘버 금지).
// ACK_BATCH_BYTES: write 콜백에서 누적 ack가 이 임계치를 넘으면 즉시 플러시(R9 배치).
// ACK_FLUSH_IDLE_MS: 잔여 ack가 이 시간 동안 더해지지 않으면 타이머 플러시(초당 ~20회 상한).
// SNAPSHOT_DRAIN_TIMEOUT_MS: snapshotAndDispose 가 미완료 write 콜백을 대기하는 한도(R11).
const ACK_BATCH_BYTES = 4 * 1024;
const ACK_FLUSH_IDLE_MS = 50;
const SNAPSHOT_DRAIN_TIMEOUT_MS = 500;

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
  // @MX:ANCHOR: [AUTO] parsedSeq — xterm write 콜백에서만 전진하는 정본 seq(R10).
  // @MX:REASON: replay_pane(paneId, parsedSeq) 와 snapshot.lastSeq 가 모두 이 값을 읽는다
  //   (fan_in >= 3: mountPane/replay, snapshotAndDispose, pty-output 간접). 수신 즉시
  //   전진하던 구 구조(결함 2)와 반대 — 파싱 완료 시점에만 움직인다. receivedSeq 는
  //   진단용(이벤트 수신 시점). IME 보류중 데이터는 여기에 아직 반영되지 않는다(R12).
  parsedSeq: number;
  /** 진단용: pty-output 이벤트 수신 시점에 전진. 정본 parsedSeq 와 분리(R10). */
  receivedSeq: number;
  resizeObserver: ResizeObserver;
  exitShown: boolean;
  /** IME output-buffering state (see writeOutput and OUTPUT_WATCHDOG_MS).
   * `imeBuffering` gates whether output is held; `outBuf`/`outBufLen` accumulate
   * the held chunks; `outWatchdog` is the force-flush timer; `lastInputTs`
   * timestamps the last onData -> PTY write for the echo pass-through window.
   * `heldMaxSeq`/`heldAckBytes` track the max seq + ack-eligible bytes across
   * the held chunks so the flush write-cb can advance parsedSeq + ack exactly
   * (R10/R12 — 보류중 데이터는 term.write 도달 전이므로 미ack). */
  imeBuffering: boolean;
  outBuf: string[];
  outBufLen: number;
  outWatchdog: ReturnType<typeof setTimeout> | null;
  lastInputTs: number;
  heldMaxSeq: number | undefined;
  heldAckBytes: number;
  // @MX:NOTE: [AUTO] ack 배치 상태(R9). write 콜백에서 ackPendingBytes 누적 →
  //   ACK_BATCH_BYTES 도달 즉시 플러시, 또는 ACK_FLUSH_IDLE_MS idle 후 플러시.
  //   ackInFlight 는 R15 late-ack (전환 전 잔여 ack invoke 완료 대기) 를 지원.
  ackPendingBytes: number;
  ackIdleTimer: ReturnType<typeof setTimeout> | null;
  ackInFlight: Set<Promise<void>>;
  /** 미완료 term.write 콜백 추적 — snapshotAndDispose 드레인(R11)이 대기하는 집합. */
  pendingDrain: Set<Promise<void>>;
  /** replay_pane invoke 진행중 플래그 — 이 팬으로 향하는 live 이벤트를 버퍼링(R10 N2). */
  replayInFlight: boolean;
  /** replay 진행중 도착한 live pty-output 이벤트의 지연 버퍼(R10 N2). */
  pendingReplayEvents: PtyOutputEvent[];
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

/** Sole output sink for PTY output and exit banners: while the pane's IME is
 * composing (see OUTPUT_WATCHDOG_MS) the data is held in a per-view buffer and
 * flushed on compositionend, so xterm's per-render textarea repositioning can't
 * corrupt the in-progress syllable. Output within ECHO_PASS_MS of the user's
 * own input passes straight through even mid-composition (it's the echo of a
 * just-committed syllable — holding it stalls the cursor and stacks the IME
 * preview over committed text). Otherwise it writes straight through.
 * Order invariant: a pass-through write always flushes the held buffer first,
 * so chunks never overtake earlier held output.
 *
 * SPEC-PTY-FLOW-001 M2: `seq` 식별 가능한 PTY 배치 이벤트일 때만 전달한다. seq 가
 * 없으면 합성 배너(exit/overflow 메시지)이므로 ack 도 parsedSeq 전진도 하지 않는다.
 * ack 는 오직 term.write 콜백(파싱 완료)에서 발생한다(R9/R12). */
export function writeOutput(
  paneId: PaneId,
  data: string,
  meta?: { seq: number; byteLen: number },
): void {
  const view = views.get(paneId);
  if (!view) return; // pane not mounted; ring buffer replay covers it later
  if (view.imeBuffering && Date.now() - view.lastInputTs > ECHO_PASS_MS) {
    appendOutput(view, data, meta);
    return;
  }
  flushOutput(view); // no-op unless an echo chunk is overtaking held output
  // @MX:ANCHOR: [AUTO] SPEC-PTY-FLOW-002 반사 ack 계약 — ack 수치는 이벤트 byteLen 에서만.
  // @MX:REASON: 백엔드가 emit 회계와 동일 원천(배너 포함 최종 문자열, R2)에서 산출한
  // byteLen 을 프론트가 그대로 반사한다. UTF-16 코드 유닛 수 기반 산정으로 되돌아가면
  // 비ASCII 에서 결손이 누적되어 emitter 가 영구 정지된다(SPEC-PTY-FLOW-002 결함).
  // autotest u8FloodAckRatio(AC-10e) 가 이 계약의 종단 가드다.
  // ackBytes: seq 가 있는 배치(실 PTY 출력)만 이벤트 byteLen 만큼 ack 누적.
  writeParsed(view, data, meta?.seq, meta === undefined ? 0 : meta.byteLen);
}

/** Hold a chunk in the view's IME buffer; arm the watchdog on the first chunk
 * and force-flush (keeping buffering armed) if the held size passes the cap.
 * seq-식별 가능한 청크는 heldMaxSeq/heldAckBytes 에도 반영하여 flush 시 정확한
 * parsedSeq 전진 + ack 를 보장한다(R12 — 보류중 데이터는 아직 term.write 도달 전). */
function appendOutput(
  view: PaneView,
  data: string,
  meta?: { seq: number; byteLen: number },
): void {
  const wasEmpty = view.outBuf.length === 0;
  view.outBuf.push(data);
  // outBufLen 은 IME 보류 버퍼 용량 캡 계산용 — ack 회계와 무관한 정당 용법(AC-4).
  view.outBufLen += data.length;
  if (meta !== undefined) {
    // R4: 보류는 여러 이벤트를 모으므로 heldAckBytes 는 개별 byteLen 의 합.
    view.heldAckBytes += meta.byteLen;
    if (view.heldMaxSeq === undefined || meta.seq > view.heldMaxSeq) {
      view.heldMaxSeq = meta.seq;
    }
  }
  if (wasEmpty) startOutputWatchdog(view);
  if (view.outBufLen > OUTPUT_BUFFER_CAP) flushOutput(view); // runaway guard
}

/** Write the held buffer to the terminal in one shot and clear it + the
 * watchdog. Leaves `imeBuffering` untouched — the caller decides whether to
 * keep buffering (size-cap flush) or stop (compositionend / blur / watchdog).
 * 누적된 heldMaxSeq/heldAckBytes 로 writeParsed 를 호출 → 콜백에서 parsedSeq 전진
 * 및 ack 누적(R9/R10). */
function flushOutput(view: PaneView): void {
  if (view.outWatchdog !== null) {
    clearTimeout(view.outWatchdog);
    view.outWatchdog = null;
  }
  if (view.outBuf.length === 0) return;
  const data = view.outBuf.join("");
  const seq = view.heldMaxSeq;
  const ackBytes = view.heldAckBytes;
  view.outBuf = [];
  view.outBufLen = 0;
  view.heldMaxSeq = undefined;
  view.heldAckBytes = 0;
  writeParsed(view, data, seq, ackBytes);
}

/** term.write(data, cb) 래퍼 — xterm 파싱 완료 시점에 단일 진실 소스를 갱신:
 *  - parsedSeq: cb 안에서만 전진(R10 정본). seq undefined 면 건드리지 않는다.
 *  - ack: ackBytes>0 일 때 ackPendingBytes 누적 → 배치 플러시 예약(R9).
 *  pendingDrain 에 promise 를 적재하여 snapshotAndDispose 드레인이 대기(R11). */
function writeParsed(
  view: PaneView,
  data: string,
  seq: number | undefined,
  ackBytes: number,
): Promise<void> {
  const p = new Promise<void>((resolve) => {
    view.term.write(data, () => {
      if (seq !== undefined && seq > view.parsedSeq) view.parsedSeq = seq;
      if (ackBytes > 0) {
        view.ackPendingBytes += ackBytes;
        scheduleAckFlush(view);
      }
      resolve();
    });
  });
  view.pendingDrain.add(p);
  p.then(
    () => {
      view.pendingDrain.delete(p);
    },
    () => {
      view.pendingDrain.delete(p);
    },
  );
  return p;
}

/** 비-PTY 기원 데이터(스냅샷 복원 / replay 응답 / 합성 배너) 기록 — ack 없이
 *  parsedSeq 만 전진(R13: replay 바이트 미ack; R10: parsedSeq는 파싱 완료 시 전진).
 *  main.ts mountPane 가 스냅샷 복원·replay 데이터 기록에 사용한다. */
export function writeParsedNoAck(
  paneId: PaneId,
  data: string,
  seq?: number,
): Promise<void> {
  const view = views.get(paneId);
  if (!view) return Promise.resolve();
  return writeParsed(view, data, seq, 0);
}

/** 누적 ack 가 배치 임계치를 넘으면 즉시 플러시; 아니면 idle 타이머로 잔여분을
 *  나중에 플러시(R9 배치 — 작은 write 마다 IPC 1회 금지). */
function scheduleAckFlush(view: PaneView): void {
  if (view.ackPendingBytes >= ACK_BATCH_BYTES) {
    void flushAckNow(view);
    return;
  }
  if (view.ackIdleTimer === null) {
    view.ackIdleTimer = setTimeout(() => {
      void flushAckNow(view);
    }, ACK_FLUSH_IDLE_MS);
  }
}

/** 누적 ack 를 ack_output 으로 플러시한다. idle 타이머 해제 후 남은분을 보내고,
 *  진행중 invoke 를 ackInFlight 에 추적하여 R15 late-ack 경로(dispose 전 await)가
 *  백엔드 도착 순서를 "마지막 ack → 리셋" 으로 고정하도록 한다. */
function flushAckNow(view: PaneView): Promise<void> {
  if (view.ackIdleTimer !== null) {
    clearTimeout(view.ackIdleTimer);
    view.ackIdleTimer = null;
  }
  if (view.ackPendingBytes <= 0) {
    // 새로 보낼 분량은 없지만 진행중 invoke 가 남아있으면 그것이라도 정착 대기.
    return Promise.allSettled([...view.ackInFlight]).then(() => undefined);
  }
  const bytes = view.ackPendingBytes;
  view.ackPendingBytes = 0;
  const p = ipc.ackOutput(view.paneId, bytes).catch((e) => console.warn("[ack]", e));
  view.ackInFlight.add(p);
  p.finally(() => {
    view.ackInFlight.delete(p);
  });
  return p;
}

/** Force-flush after OUTPUT_WATCHDOG_MS so a composition left open (no
 * compositionend) can't stall output indefinitely; give up buffering for the
 * rest of this composition (a fresh compositionstart re-arms it). */
function startOutputWatchdog(view: PaneView): void {
  if (view.outWatchdog !== null) return;
  view.outWatchdog = setTimeout(() => {
    view.outWatchdog = null;
    flushOutput(view);
    view.imeBuffering = false;
  }, OUTPUT_WATCHDOG_MS);
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
    parsedSeq: 0,
    receivedSeq: 0,
    resizeObserver: new ResizeObserver(() => syncSize(view)),
    exitShown: false,
    imeBuffering: false,
    outBuf: [],
    outBufLen: 0,
    outWatchdog: null,
    lastInputTs: 0,
    heldMaxSeq: undefined,
    heldAckBytes: 0,
    ackPendingBytes: 0,
    ackIdleTimer: null,
    ackInFlight: new Set(),
    pendingDrain: new Set(),
    replayInFlight: false,
    pendingReplayEvents: [],
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
    view.lastInputTs = Date.now(); // opens the echo pass-through window (writeOutput)
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
    view.imeBuffering = true; // hold output so renders can't move the IME textarea
  });
  term.textarea?.addEventListener("compositionend", () => {
    composing = false;
    awaitingComposedData = true;
    // Composition done: release the held output in one write and stop buffering.
    view.imeBuffering = false;
    flushOutput(view);
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
  // Blur defense: if the textarea loses focus mid-composition the browser may
  // never fire compositionend, leaving composing/awaitingComposedData stuck true
  // — which would kill the Ctrl+C/V shortcuts (they bail while composing). Reset
  // the composition flags, drop any pending newline (do NOT send it — the user
  // moved focus, not committed), and release the held output.
  term.textarea?.addEventListener("blur", () => {
    composing = false;
    awaitingComposedData = false;
    pendingNewline = false;
    view.imeBuffering = false;
    flushOutput(view);
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
 * (unmount policy for inactive workspaces, ADR-002).
 *
 * SPEC-PTY-FLOW-001 M2 (R11/R15): async. ① IME 보류 버퍼 플러시 → ② 미완료 write
 * 콜백을 SNAPSHOT_DRAIN_TIMEOUT_MS 한도로 대기(드레인) → ③ 잔여 ack 배치 플러시+대기 →
 * ④ serialize. 타임아웃 시에도 정확성 유지 — parsedSeq 는 파싱된 범위만 가리키므로
 * remount 후 replay_pane(parsedSeq) 가 공백을 채운다(R11 핵심 역전). ③ 은 R15 late-ack:
 * 전환 리셋 전에 마지막 ack 가 백엔드에 도착하도록 invoke 완료를 await 한다. */
export async function snapshotAndDispose(paneId: PaneId): Promise<void> {
  const view = views.get(paneId);
  if (!view) return;
  // Flush any IME-held output into the terminal so it lands in the snapshot;
  // otherwise buffered chunks would be lost on unmount.
  view.imeBuffering = false;
  flushOutput(view);
  // @MX:NOTE: [AUTO] 드레인 — 미완료 write 콜백 대기(R11). 직렬화 전 파싱 상태를
  //   확정하여 스냅샷이 파싱 완료분까지 포함하도록 한다.
  const drain = Promise.all([...view.pendingDrain]).then(
    () => undefined,
    () => undefined,
  );
  await Promise.race([
    drain,
    new Promise<void>((r) => setTimeout(r, SNAPSHOT_DRAIN_TIMEOUT_MS)),
  ]);
  // @MX:WARN: [AUTO] R15 late-ack — serialize 전 잔여 ack invoke 를 반드시 await.
  // @MX:REASON: await 없으면 전환 리셋(replay_synced=false)보다 늦게 도착한 옛 ack 가
  //   acked_bytes 를 영구 부풀려 outstanding 왜곡 → emitter 게이트 영향. "마지막 ack →
  //   리셋" 도착 순서를 고정하는 유일한 수단이다.
  await flushAckNow(view);
  try {
    snapshots.set(paneId, {
      data: view.serialize.serialize({ scrollback: 1000 }),
      lastSeq: view.parsedSeq,
    });
  } catch (e) {
    console.warn("[terminal-f] serialize failed", e);
    snapshots.set(paneId, { data: "", lastSeq: view.parsedSeq });
  }
  disposeView(paneId);
}

export function disposeView(paneId: PaneId): void {
  const view = views.get(paneId);
  if (!view) return;
  if (view.outWatchdog !== null) clearTimeout(view.outWatchdog); // stop the IME flush timer
  if (view.ackIdleTimer !== null) clearTimeout(view.ackIdleTimer); // stop the ack idle flush
  view.outBuf = []; // discard any held output; the terminal is going away
  view.outBufLen = 0;
  view.heldMaxSeq = undefined;
  view.heldAckBytes = 0;
  view.ackPendingBytes = 0;
  view.pendingReplayEvents = []; // discard buffered events; pane is gone (R10)
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
