// Shared IPC types. Must stay in sync with the Rust models in
// src-tauri/src/{model,session,commands,output}.rs (serde camelCase).

export type Direction = "row" | "column";
export type PaneId = string;
export type WorkspaceId = string;
export type SessionId = string;

export interface PaneLeaf {
  kind: "pane";
  id: PaneId;
  sessionId: SessionId | null;
  cwd: string;
  command: string | null;
  labels: string[];
  allowInjection: boolean;
  allowObserve: boolean;
  startupCommand?: string | null;
}

export interface SplitNode {
  kind: "split";
  id: string;
  direction: Direction;
  ratio: number;
  first: PaneNode;
  second: PaneNode;
}

export type PaneNode = PaneLeaf | SplitNode;

export interface Workspace {
  id: WorkspaceId;
  name: string;
  root: PaneNode;
  activePaneId: PaneId | null;
  createdAt: number;
  updatedAt: number;
  color?: string | null;
  /** 워크스페이스 시작 폴더. `root`(팬 트리)와 혼동하지 말 것. */
  rootDir?: string | null;
}

export interface WorkspaceMeta {
  id: WorkspaceId;
  name: string;
  color: string | null;
  /** 시작 폴더. 비활성 워크스페이스도 메뉴에서 현재 경로/Clear 표시에 사용 (R11). */
  rootDir: string | null;
}

export interface WorkspaceActivity {
  workspaceId: WorkspaceId;
  unseenOutput: boolean;
  exitedPanes: number;
  livePanes: number;
}

export interface UiPrefs {
  theme?: string;
  fontSize?: number;
  sidebar?: { collapsed?: boolean; width?: number };
  /** Copy selection to clipboard automatically on select (default off). */
  copyOnSelect?: boolean;
  /** Ctrl/Cmd+click a URL in output to open it in the browser (default on). */
  openUrlOnClick?: boolean;
}

export type Lifecycle = "starting" | "running" | "exited" | "closing";

export interface SessionInfo {
  paneId: PaneId;
  sessionId: SessionId;
  workspaceId: WorkspaceId;
  state: Lifecycle;
  exitCode: number | null;
  command: string;
}

export interface AppSnapshot {
  workspaces: WorkspaceMeta[];
  activeWorkspaceId: WorkspaceId | null;
  ui: UiPrefs & Record<string, unknown>;
}

export interface WorkspaceResult {
  workspace: Workspace;
  sessions: SessionInfo[];
  warnings: string[];
}

export interface CreateWorkspaceResult {
  workspace: Workspace;
  warning: string | null;
  workspaces: WorkspaceMeta[];
}

export interface SetWorkspaceRootResult {
  workspaces: WorkspaceMeta[];
  workspace: Workspace;
  rewritten: number;
}

export interface ReplayResult {
  data: string;
  lastSeq: number;
  dropped: boolean;
  sessionId: SessionId;
  state: Lifecycle;
}

export interface BootInfo {
  autotest: boolean;
  shell: string | null;
}

export interface MemoryStats {
  rssBytes: number;
  liveSessions: number;
}

// SPEC-PTY-FLOW-001 R1 — 흐름 제어 관측 창구(flow_stats 커맨드 원천).
// 백엔드 원자 변수의 유일한 프론트 관측 경로. autotest flood 체크(AC-9)가
// outstanding/emitterPaused/readerParked 를 기계 판정하는 데 사용한다.
export interface FlowStats {
  emitted: number;
  acked: number;
  outstanding: number;
  emitterPaused: boolean;
  readerParked: boolean;
  // SPEC-PTY-FLOW-002 R10 — 두 밸브 카운터 신규 노출 (기존 5필드 불변, append-only).
  valveFired: number;
  emitterValveFired: number;
}

export interface InjectReceipt {
  paneId: PaneId;
  sessionId: SessionId;
  bytes: number;
  bracketed: boolean;
  submitted: boolean;
}

export interface AuditEntry {
  ts: number;
  source: string;
  workspaceId: WorkspaceId;
  paneId: PaneId;
  sessionId: SessionId;
  bytes: number;
  submitted: boolean;
  bracketed: boolean;
  preview: string;
}

export type RuleMode = "confirm" | "auto";

export type RuleSource =
  | { type: "gitDiff"; repo: string }
  | { type: "timer"; everyMs: number };

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  /** Legacy git repo path; new rules set `source` instead. */
  repo: string;
  source: RuleSource | null;
  cooldownMs: number;
  maxPerMin: number;
  targetLabel: string | null;
  targetPane: string | null;
  template: string;
  submit: boolean;
  requireIdle: boolean;
  mode: RuleMode;
}

// Phase B templates
export interface TemplateParam {
  name: string;
  prompt?: string | null;
  default?: string | null;
  kind?: string | null;
}

export interface TemplatePane {
  kind: "pane";
  cwd?: string | null;
  labels?: string[];
  allowInjection?: boolean;
  allowObserve?: boolean;
  command?: string | null;
  startupCommand?: string | null;
}

export interface TemplateSplit {
  kind: "split";
  direction: Direction;
  ratio: number;
  first: TemplateNode;
  second: TemplateNode;
}

export type TemplateNode = TemplatePane | TemplateSplit;

export interface Template {
  name: string;
  params: TemplateParam[];
  root: TemplateNode;
}

export interface RepoProfile {
  template: Template;
  hasCommands: boolean;
  trusted: boolean;
  repo: string;
}

export interface Proposal {
  id: string;
  ruleId: string;
  ruleName: string;
  targetLabel: string | null;
  targetPane: string | null;
  text: string;
  submit: boolean;
  requireIdle: boolean;
  summary: string;
}

export interface PtyOutputEvent {
  workspaceId: WorkspaceId;
  paneId: PaneId;
  sessionId: SessionId;
  seq: number;
  data: string;
  /** SPEC-PTY-FLOW-002 R2 — 배너 포함 최종 data 의 UTF-8 바이트 길이(백엔드 단일 원천). */
  byteLen: number;
}

export interface PtyExitEvent {
  workspaceId: WorkspaceId;
  paneId: PaneId;
  sessionId: SessionId;
  exitCode: number | null;
}

export function collectLeaves(node: PaneNode, out: PaneLeaf[] = []): PaneLeaf[] {
  if (node.kind === "pane") {
    out.push(node);
  } else {
    collectLeaves(node.first, out);
    collectLeaves(node.second, out);
  }
  return out;
}

export function firstPaneId(node: PaneNode): PaneId {
  return node.kind === "pane" ? node.id : firstPaneId(node.first);
}
