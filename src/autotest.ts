// Scripted in-app smoke test + benchmark (enabled with TERMF_AUTOTEST=1).
//
// Drives the real UI end-to-end: split, workspace create/switch, PTY echo,
// keep-alive across switches, switch latency percentiles, and a short soak.
// Writes its report through the backend (autotest_report) and exits the app.

import * as ipc from "./ipc";
import * as terms from "./terms";
import { percentile, sleep } from "./util";

export interface AutotestCtl {
  currentWorkspaceId(): string;
  activePaneId(): string;
  paneIds(): string[];
  switchTo(workspaceId: string): Promise<number>;
  splitActive(direction: "row" | "column"): Promise<void>;
  addWorkspace(): Promise<string>;
  readBuffer(paneId: string): string;
  lastSwitchMs(): number;
}

interface Report {
  startedAt: string;
  steps: string[];
  errors: string[];
  checks: Record<string, boolean>;
  switchLatencyMs?: { samples: number; p50: number; p95: number; max: number };
  soak?: {
    seconds: number;
    rssBeforeBytes: number;
    rssAfterBytes: number;
    growthFactor: number;
  };
  ok?: boolean;
}

export async function runAutotest(ctl: AutotestCtl): Promise<void> {
  const report: Report = {
    startedAt: new Date().toISOString(),
    steps: [],
    errors: [],
    checks: {},
  };
  const step = (name: string) => {
    report.steps.push(name);
    console.log(`[autotest] ${name}`);
  };

  try {
    const ws1 = ctl.currentWorkspaceId();

    // -- pane split in ws1 -------------------------------------------------
    // Relative check: the loaded config may already contain panes from a
    // previous run (config persistence is a feature, not a fixture).
    const panesBefore = ctl.paneIds().length;
    await ctl.splitActive("row");
    step("split-row-ws1");
    report.checks.splitCreatedPane = ctl.paneIds().length === panesBefore + 1;

    // -- workspace create + split ------------------------------------------
    const ws2 = await ctl.addWorkspace();
    step("create-and-switch-ws2");
    await ctl.splitActive("column");
    step("split-column-ws2");
    await sleep(3000); // let shells print their prompts

    // -- echo I/O check in ws2 ----------------------------------------------
    const echoPane = ctl.activePaneId();
    await ipc.writePane(echoPane, "echo TERMF_ECHO_OK\r");
    await sleep(2000);
    report.checks.echo = ctl.readBuffer(echoPane).includes("TERMF_ECHO_OK");
    step(`echo-check:${report.checks.echo}`);

    // -- injection machinery (M2.0) ------------------------------------------
    const injPane = ctl.activePaneId();
    // allowlist default: refused before opt-in
    let refused = false;
    try {
      await ipc.injectPrompt({ paneId: injPane, text: "echo NOPE" });
    } catch {
      refused = true;
    }
    report.checks.injectRefusedByDefault = refused;
    // opt in, then inject (retry while the idle gate reports busy)
    await ipc.setPaneInjection(ctl.currentWorkspaceId(), injPane, true);
    let injected = false;
    for (let i = 0; i < 40 && !injected; i++) {
      try {
        await ipc.injectPrompt({ paneId: injPane, text: "echo TERMF_INJECTED" });
        injected = true;
      } catch (e) {
        if (!String(e).includes("busy")) throw e;
        await sleep(300);
      }
    }
    await sleep(1500);
    report.checks.inject = injected && ctl.readBuffer(injPane).includes("TERMF_INJECTED");
    const audit = await ipc.readAudit(5);
    report.checks.auditLogged = audit.some((a) => a.preview.includes("TERMF_INJECTED"));
    step(
      `inject refused-by-default:${report.checks.injectRefusedByDefault} ok:${report.checks.inject} audit:${report.checks.auditLogged}`,
    );

    // -- automation rule engine (M2.1) ---------------------------------------
    // Use a run-unique label so accumulated config from prior runs (autotest
    // is not hermetic — config persists) can't make the target ambiguous.
    const injectLabel = `codex-${Date.now()}`;
    await ipc.setPaneLabels(ctl.currentWorkspaceId(), injPane, [injectLabel]);
    const ruleId = "autotest-rule";
    await ipc.upsertRule({
      id: ruleId,
      name: "autotest git review",
      enabled: true,
      repo: ".", // not a git repo under test harness; run_rule_now uses fallback
      source: null, // exercise the legacy repo->gitDiff fallback path
      cooldownMs: 0,
      maxPerMin: 99,
      targetLabel: injectLabel,
      targetPane: null,
      template: "AUTOMATION {{summary}}",
      submit: true,
      requireIdle: true,
      mode: "confirm",
    });
    const rules = await ipc.listRules();
    report.checks.ruleStored = rules.some((r) => r.id === ruleId);

    // Confirm mode: run-now should create a pending proposal, not inject yet.
    await ipc.runRuleNow(ruleId);
    await sleep(300);
    const proposals = await ipc.listProposals();
    const mine = proposals.find((p) => p.ruleId === ruleId);
    report.checks.proposalCreated = !!mine;
    const beforeApprove = ctl.readBuffer(injPane).includes("AUTOMATION");
    // Approve -> injection runs via the shared gated path (retry on idle-busy).
    let approved = false;
    if (mine) {
      for (let i = 0; i < 40 && !approved; i++) {
        try {
          await ipc.resolveProposal(mine.id, true);
          approved = true;
        } catch (e) {
          if (!String(e).includes("busy")) throw e;
          await sleep(300);
        }
      }
    }
    await sleep(1500);
    report.checks.proposalNotInjectedUntilApproved = !beforeApprove;
    report.checks.approvedInjection =
      approved && ctl.readBuffer(injPane).includes("AUTOMATION");
    const audit2 = await ipc.readAudit(10);
    report.checks.ruleAuditSource = audit2.some((a) => a.source === ruleId);
    await ipc.removeRule(ruleId);

    // timer source (M2.1.5): auto mode + run-now injects immediately.
    const timerId = "autotest-timer";
    await ipc.upsertRule({
      id: timerId,
      name: "autotest timer",
      enabled: true,
      repo: "",
      source: { type: "timer", everyMs: 60_000 },
      cooldownMs: 0,
      maxPerMin: 99,
      targetLabel: injectLabel,
      targetPane: null,
      template: "TIMER_TICK",
      submit: true,
      requireIdle: true,
      mode: "auto",
    });
    let timerInjected = false;
    for (let i = 0; i < 40 && !timerInjected; i++) {
      try {
        await ipc.runRuleNow(timerId); // auto mode -> injects directly
        timerInjected = true;
      } catch (e) {
        if (!String(e).includes("busy")) throw e;
        await sleep(300);
      }
    }
    await sleep(1500);
    report.checks.timerRuleInjected =
      timerInjected && ctl.readBuffer(injPane).includes("TIMER_TICK");
    await ipc.removeRule(timerId);

    // -- output observation / spool (M2.2) -----------------------------------
    // Reading before opt-in is refused; after opt-in the spool captures output.
    let observeRefused = false;
    try {
      await ipc.readPaneOutput(injPane, 0);
    } catch {
      observeRefused = true;
    }
    report.checks.observeRefusedByDefault = observeRefused;
    await ipc.setPaneObserve(ctl.currentWorkspaceId(), injPane, true);
    await ipc.writePane(injPane, "echo TERMF_OBSERVE_OK\r");
    await sleep(1500);
    const out = await ipc.readPaneOutput(injPane, 0).catch(() => ({ data: "", offset: 0, total: 0 }));
    report.checks.observeReadsOutput = out.data.includes("TERMF_OBSERVE_OK");
    step(
      `automation stored:${report.checks.ruleStored} proposal:${report.checks.proposalCreated} approved:${report.checks.approvedInjection} auditSrc:${report.checks.ruleAuditSource} timer:${report.checks.timerRuleInjected} observeGate:${report.checks.observeRefusedByDefault} observeRead:${report.checks.observeReadsOutput}`,
    );

    // -- keep-alive across workspace switch ---------------------------------
    await ctl.switchTo(ws1);
    const tickPane = ctl.activePaneId();
    await ipc.writePane(
      tickPane,
      '1..2000 | ForEach-Object { "TICK $_"; Start-Sleep -Milliseconds 200 }\r',
    );
    await sleep(1500);
    const maxTick = (paneText: string): number => {
      const ticks = [...paneText.matchAll(/TICK (\d+)/g)].map((m) => Number(m[1]));
      return ticks.length ? Math.max(...ticks) : 0;
    };
    const tickBefore = maxTick(ctl.readBuffer(tickPane));
    await ctl.switchTo(ws2);
    await sleep(5000); // ws1 is inactive; its process must keep running
    await ctl.switchTo(ws1);
    await sleep(500);
    const tickAfter = maxTick(ctl.readBuffer(tickPane));
    report.checks.keepAlive = tickAfter > tickBefore && tickBefore > 0;
    step(`keep-alive:${report.checks.keepAlive} (tick ${tickBefore} -> ${tickAfter})`);

    // -- workspace switch latency -------------------------------------------
    const latencies: number[] = [];
    for (let i = 0; i < 30; i++) {
      latencies.push(await ctl.switchTo(i % 2 === 0 ? ws2 : ws1));
    }
    report.switchLatencyMs = {
      samples: latencies.length,
      p50: percentile(latencies, 0.5),
      p95: percentile(latencies, 0.95),
      max: Math.max(...latencies),
    };
    step(`switch-latency p95=${report.switchLatencyMs.p95.toFixed(1)}ms`);
    report.checks.switchP95Under150 = report.switchLatencyMs.p95 < 150;

    // -- short soak (backend RSS while output keeps flowing) -----------------
    const mem0 = await ipc.memoryStats();
    const soakStart = performance.now();
    for (let i = 0; i < 9; i++) {
      await sleep(5000);
      await ctl.switchTo(i % 2 === 0 ? ws1 : ws2);
    }
    const soakSecs = (performance.now() - soakStart) / 1000;
    const mem1 = await ipc.memoryStats();
    report.soak = {
      seconds: Math.round(soakSecs),
      rssBeforeBytes: mem0.rssBytes,
      rssAfterBytes: mem1.rssBytes,
      growthFactor: mem0.rssBytes > 0 ? mem1.rssBytes / mem0.rssBytes : 0,
    };
    step(`soak ${report.soak.seconds}s rss x${report.soak.growthFactor.toFixed(3)}`);

    // -- template apply (Phase B) --------------------------------------------
    const tplLabel = `tpl-${Date.now()}`;
    const wsBefore = (await ipc.getState()).workspaces.length;
    const applySnap = await ipc.applyTemplate(
      {
        name: "autotest-template",
        params: [{ name: "marker", prompt: "marker", default: null, kind: "text" }],
        root: {
          kind: "split",
          direction: "row",
          ratio: 0.5,
          first: { kind: "pane", labels: [tplLabel] },
          second: {
            kind: "pane",
            startupCommand: "echo ${marker}",
          },
        },
      },
      { marker: "TEMPLATE_STARTUP_OK" },
    );
    report.checks.templateCreatedWorkspace =
      applySnap.workspaces.length === wsBefore + 1;
    const tplWs = applySnap.workspaces[applySnap.workspaces.length - 1];
    await ctl.switchTo(tplWs.id);
    report.checks.templateTwoPanes = ctl.paneIds().length === 2;
    // startupCommand runs after the shell settles (~800ms idle) then echoes.
    await sleep(6000);
    const anyStartup = ctl.paneIds().some((id) => ctl.readBuffer(id).includes("TEMPLATE_STARTUP_OK"));
    report.checks.templateStartupRan = anyStartup;
    step(
      `template ws:${report.checks.templateCreatedWorkspace} panes:${report.checks.templateTwoPanes} startup:${report.checks.templateStartupRan}`,
    );

    // -- image paste bridge (ADR-010) ----------------------------------------
    // Synthetic image-only paste on a pane: the capture listener must save the
    // image via the backend and paste the file path into the shell input.
    const pastePane = ctl.activePaneId();
    const pngB64 =
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    const pngBytes = Uint8Array.from(atob(pngB64), (c) => c.charCodeAt(0));
    const imgFile = new File([pngBytes], "shot.png", { type: "image/png" });
    // Isolate the backend save path first (also proves the IPC command).
    let backendPath = "";
    try {
      backendPath = await ipc.savePastedImage(pngB64, "image/png");
    } catch (e) {
      step(`paste-backend-error:${e}`);
    }
    step(`paste-backend-path:${backendPath.length > 0 ? "ok" : "EMPTY"}`);
    let pasteVia = "none";
    try {
      const dt = new DataTransfer();
      dt.items.add(imgFile);
      const view = terms.getView(pastePane);
      const target = view?.term.textarea ?? view?.el;
      if (target) {
        // Chromium ignores the ClipboardEventInit.clipboardData member on
        // synthetic events; attach it via defineProperty instead.
        const ev = new ClipboardEvent("paste", { bubbles: true, cancelable: true });
        Object.defineProperty(ev, "clipboardData", { value: dt });
        target.dispatchEvent(ev);
        pasteVia = "event";
      }
    } catch {
      /* ClipboardEvent construction unsupported; fall back below */
    }
    const pathInBuffer = async (): Promise<boolean> => {
      for (let i = 0; i < 20; i++) {
        // The pasted path can wrap across terminal lines; flatten first.
        const flat = ctl.readBuffer(pastePane).replace(/[\r\n]+/g, "");
        if (/img-\d+[^"]*\.png/.test(flat)) return true;
        await sleep(200);
      }
      return false;
    };
    let pasted = await pathInBuffer();
    if (!pasted) {
      pasteVia = `${pasteVia}+direct`;
      await terms.pasteImageBlob(pastePane, imgFile);
      pasted = await pathInBuffer();
    }
    report.checks.pasteImage = pasted;
    step(`paste-image:${pasted} via:${pasteVia}`);

    // Drop-path bridge: quoted dropped paths must land in the pane input.
    const dropOk = terms.pastePathsToPane(pastePane, ["C:\\termf drop\\pic 1.png"]);
    let dropSeen = false;
    for (let i = 0; i < 15 && !dropSeen; i++) {
      dropSeen = ctl.readBuffer(pastePane).replace(/[\r\n]+/g, "").includes("termf drop");
      if (!dropSeen) await sleep(200);
    }
    report.checks.pasteDrop = dropOk && dropSeen;
    // Direct OS clipboard read (Ctrl+V path). Read-only: no term.paste here,
    // since the machine clipboard could hold multi-line text that a live
    // shell would execute. Content varies; only the call must succeed.
    let clipKind = "error";
    try {
      clipKind = (await ipc.pasteClipboard()).kind;
    } catch {
      /* recorded as error */
    }
    report.checks.pasteClipboardRead = clipKind !== "error";
    step(`paste-drop:${report.checks.pasteDrop} clip-read:${clipKind}`);

    // -- workspace delete ----------------------------------------------------
    const before = (await ipc.getState()).workspaces.length;
    const res = await ipc.createWorkspace();
    const after = await ipc.deleteWorkspace(res.workspace.id);
    report.checks.workspaceCrud = after.workspaces.length === before;
    step(`workspace-crud:${report.checks.workspaceCrud}`);
  } catch (e) {
    report.errors.push(String(e));
    console.error("[autotest] error", e);
  }

  report.ok =
    report.errors.length === 0 &&
    report.checks.echo === true &&
    report.checks.keepAlive === true &&
    report.checks.splitCreatedPane === true &&
    report.checks.injectRefusedByDefault === true &&
    report.checks.inject === true &&
    report.checks.auditLogged === true &&
    report.checks.proposalCreated === true &&
    report.checks.approvedInjection === true &&
    report.checks.ruleAuditSource === true &&
    report.checks.timerRuleInjected === true &&
    report.checks.observeRefusedByDefault === true &&
    report.checks.observeReadsOutput === true &&
    report.checks.templateCreatedWorkspace === true &&
    report.checks.templateTwoPanes === true &&
    report.checks.templateStartupRan === true &&
    report.checks.pasteImage === true &&
    report.checks.pasteDrop === true &&
    report.checks.pasteClipboardRead === true;

  try {
    await ipc.autotestReport(report);
  } catch (e) {
    console.error("[autotest] report write failed", e);
  }
  await ipc.exitApp(report.ok ? 0 : 1);
}
