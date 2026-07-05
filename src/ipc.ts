import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSnapshot,
  AuditEntry,
  BootInfo,
  InjectReceipt,
  Proposal,
  RepoProfile,
  Rule,
  Template,
  CreateWorkspaceResult,
  Direction,
  MemoryStats,
  PtyExitEvent,
  PtyOutputEvent,
  ReplayResult,
  WorkspaceActivity,
  WorkspaceMeta,
  WorkspaceResult,
} from "./types";

export const getState = () => invoke<AppSnapshot>("get_state");
export const workspaceActivity = () => invoke<WorkspaceActivity[]>("workspace_activity");
export const reorderWorkspaces = (workspaceIds: string[]) =>
  invoke<WorkspaceMeta[]>("reorder_workspaces", { workspaceIds });
export const setWorkspaceColor = (workspaceId: string, color: string | null) =>
  invoke<WorkspaceMeta[]>("set_workspace_color", { workspaceId, color });
export const setUiPrefs = (ui: unknown) => invoke<void>("set_ui_prefs", { ui });
export const listWorkspaces = () => invoke<WorkspaceMeta[]>("list_workspaces");
export const createWorkspace = (name?: string) =>
  invoke<CreateWorkspaceResult>("create_workspace", { name: name ?? null });
export const renameWorkspace = (workspaceId: string, name: string) =>
  invoke<WorkspaceMeta[]>("rename_workspace", { workspaceId, name });
export const deleteWorkspace = (workspaceId: string) =>
  invoke<AppSnapshot>("delete_workspace", { workspaceId });
export const switchWorkspace = (workspaceId: string) =>
  invoke<WorkspaceResult>("switch_workspace", { workspaceId });

export const splitPane = (workspaceId: string, paneId: string, direction: Direction) =>
  invoke<WorkspaceResult>("split_pane", { workspaceId, paneId, direction });
export const closePane = (workspaceId: string, paneId: string) =>
  invoke<WorkspaceResult>("close_pane", { workspaceId, paneId });
export const resizeSplit = (workspaceId: string, splitId: string, ratio: number) =>
  invoke<number>("resize_split", { workspaceId, splitId, ratio });
export const setActivePane = (workspaceId: string, paneId: string) =>
  invoke<void>("set_active_pane", { workspaceId, paneId });

// stdin injection: an explicit paneId is always required (never "current").
export const writePane = (paneId: string, data: string) =>
  invoke<void>("write_pane", { paneId, data });
// TODO(M1): broadcastWrite(paneIds, data) — intentionally not implemented in M0.

// M2.0 injection machinery (allowlist + idle gate + audit on the backend)
export const setPaneLabels = (workspaceId: string, paneId: string, labels: string[]) =>
  invoke<WorkspaceResult>("set_pane_labels", { workspaceId, paneId, labels });
export const setPaneInjection = (workspaceId: string, paneId: string, allow: boolean) =>
  invoke<WorkspaceResult>("set_pane_injection", { workspaceId, paneId, allow });
export const setPaneObserve = (workspaceId: string, paneId: string, allow: boolean) =>
  invoke<WorkspaceResult>("set_pane_observe", { workspaceId, paneId, allow });
export const readPaneOutput = (paneId: string, from: number, max?: number) =>
  invoke<{ data: string; offset: number; total: number }>("read_pane_output", {
    paneId,
    from,
    max: max ?? null,
  });
export const controlApiInfo = () =>
  invoke<{ pipeName: string; infoPath: string }>("control_api_info");

// Phase B templates
export const applyTemplate = (template: Template, params?: Record<string, string>) =>
  invoke<AppSnapshot>("apply_template", { template, params: params ?? null });
export const listTemplates = () => invoke<string[]>("list_templates");
export const getTemplate = (name: string) => invoke<Template>("get_template", { name });
export const saveTemplate = (template: Template) => invoke<string[]>("save_template", { template });
export const deleteTemplate = (name: string) => invoke<string[]>("delete_template", { name });
export const workspaceAsTemplate = (workspaceId: string, name: string) =>
  invoke<Template>("workspace_as_template", { workspaceId, name });
export const readRepoProfile = (repo: string) =>
  invoke<RepoProfile>("read_repo_profile", { repo });
export const trustRepo = (repo: string) => invoke<void>("trust_repo", { repo });

// Image paste bridge (ADR-010): save clipboard image bytes, get a file path.
export const savePastedImage = (dataBase64: string, mime?: string) =>
  invoke<string>("save_pasted_image", { dataBase64, mime: mime ?? null });
// Direct OS clipboard read for Ctrl+V (xterm swallows the browser paste event).
export const pasteClipboard = () =>
  invoke<{ kind: "text" | "imagePath"; data: string } | { kind: "none" }>("paste_clipboard");
// Smart-copy: write selected terminal text to the OS clipboard (Ctrl+C with a
// selection / Ctrl+Shift+C / right-click copy). Written in Rust via arboard.
export const copyToClipboard = (text: string) => invoke<void>("copy_to_clipboard", { text });

// Open an http/https URL (Ctrl+click on a linkified URL in terminal output) in
// the OS default browser. The backend validates the scheme and rejects anything
// that is not http(s), so this rejects for e.g. javascript:/file: URLs.
export const openExternalUrl = (url: string) => invoke<void>("open_external_url", { url });

// Opt-in pwsh $PROFILE shell integration: status + install for a named feature
// ("multiline" = Alt+Enter->AddLine, "cwd" = OSC 9;9 prompt reporter). Install
// edits the user's $PROFILE, so the UI confirms with the snippet first.
export type ShellIntegrationFeature = "multiline" | "cwd";
export interface PwshIntegrationInfo {
  profilePath: string | null;
  installed: boolean;
  /** True when the installed block matches the current snippet; false when an
   *  older block is present and re-running would refresh it. */
  upToDate: boolean;
  snippet: string;
  available: boolean;
}
export const pwshIntegrationStatus = (feature: ShellIntegrationFeature) =>
  invoke<PwshIntegrationInfo>("pwsh_integration_status", { feature });
export const installPwshIntegration = (feature: ShellIntegrationFeature) =>
  invoke<PwshIntegrationInfo>("install_pwsh_integration", { feature });
export const injectPrompt = (opts: {
  paneId?: string;
  label?: string;
  text: string;
  submit?: boolean;
  requireIdle?: boolean;
}) =>
  invoke<InjectReceipt>("inject_prompt", {
    paneId: opts.paneId ?? null,
    label: opts.label ?? null,
    text: opts.text,
    submit: opts.submit ?? true,
    requireIdle: opts.requireIdle ?? true,
  });
export const setInjectionPaused = (paused: boolean) =>
  invoke<boolean>("set_injection_paused", { paused });
export const injectionStatus = () => invoke<boolean>("injection_status");
export const readAudit = (limit?: number) =>
  invoke<AuditEntry[]>("read_audit", { limit: limit ?? null });

// M2.1 automation rule engine
export const listRules = () => invoke<Rule[]>("list_rules");
export const upsertRule = (rule: Rule) => invoke<Rule[]>("upsert_rule", { rule });
export const removeRule = (ruleId: string) => invoke<Rule[]>("remove_rule", { ruleId });
export const setRuleEnabled = (ruleId: string, enabled: boolean) =>
  invoke<Rule[]>("set_rule_enabled", { ruleId, enabled });
export const listProposals = () => invoke<Proposal[]>("list_proposals");
export const resolveProposal = (proposalId: string, approve: boolean) =>
  invoke<InjectReceipt | null>("resolve_proposal", { proposalId, approve });
export const runRuleNow = (ruleId: string) => invoke<string>("run_rule_now", { ruleId });

export const onAutomationProposal = (cb: (p: Proposal) => void): Promise<UnlistenFn> =>
  listen<Proposal>("automation-proposal", (e) => cb(e.payload));
export const resizePty = (paneId: string, rows: number, cols: number) =>
  invoke<void>("resize_pty", { paneId, rows, cols });
export const replayPane = (paneId: string, fromSeq: number) =>
  invoke<ReplayResult>("replay_pane", { paneId, fromSeq });

export const getBootInfo = () => invoke<BootInfo>("get_boot_info");
export const memoryStats = () => invoke<MemoryStats>("memory_stats");
export const autotestReport = (report: unknown) =>
  invoke<string>("autotest_report", { report });
export const exitApp = (code: number) => invoke<void>("exit_app", { code });

export const onPtyOutput = (cb: (ev: PtyOutputEvent) => void): Promise<UnlistenFn> =>
  listen<PtyOutputEvent>("pty-output", (e) => cb(e.payload));
export const onPtyExit = (cb: (ev: PtyExitEvent) => void): Promise<UnlistenFn> =>
  listen<PtyExitEvent>("pty-exit", (e) => cb(e.payload));
