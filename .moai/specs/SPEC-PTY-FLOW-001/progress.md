# SPEC-PTY-FLOW-001 — progress

## §E.1 Plan-phase Audit-Ready Signal

plan_status: audit-ready
plan_complete_at: 2026-08-12
artifacts: spec.md (GEARS R1~R16) + plan.md (M1~M3, Tier M) + acceptance.md (AC-1~AC-15, sub-ID 포함 17항목)
plan_audit: iteration 1 findings D1~D10 반영 (spec 0.2.0)

## §E.2 Run-phase Evidence

Phase 1 Plan Audit Gate (re-run on v0.2.1, 2026-08-12): **PASS, 0.92** (Tier M threshold 0.80). v0.2.1 N1/N2/N3 assessed SOUND. 6 findings, all MINOR/NIT — non-blocking: D1 (R15:152 "영구 부풀린다" wording → "일시 과소 산정"), D2 (R15 frontend obligation buried — surfaced in M2 delegation as mitigation), D3 (R10 5-obligation density), D4 (AC-10d advisory for N2 replay-deferral — verification baked into M3), D5 (AC-4 phrasing NIT), D6 (R10 defer-buffer bound NIT). Spec-body wording fixes (D1/D2/D4 formal) deferred to sync-phase manager-spec touch. Report: `.moai/reports/plan-audit/SPEC-PTY-FLOW-001-2026-08-12.md`.

## §E.3 Run-phase Audit-Ready Signal

_<pending run-phase>_

## §E.4 Sync-phase Audit-Ready Signal

_<pending sync-phase>_

## §F Phase 4 Mode Selection

- tier: M | scope: ~7 files (session.rs/output.rs/commands.rs/lib.rs + terms.ts/main.ts/ipc.ts + autotest/bench) | domains: 4 (Rust backend, TS frontend, autotest, bench/docs) | concurrency benefit: LOW (coding-heavy, new concurrency logic)
- Mode 1 trivial: no | Mode 2 background: no (write work) | Mode 3 agent-team: RETIRED | Mode 4 parallel: no (coding-heavy → Mode 5 per Anthropic coding-task parallelism caveat) | Mode 6 workflow: no (semantic new logic + inter-file deps, not high-volume mechanical) | Mode 5 sub-agent: **SELECTED**
- Decision: `sub-agent` (sequential, per-milestone M1→M2→M3)
- Justification: Tier M coding-heavy SPEC with inter-file dependencies and concurrency-sensitive new logic (flow-control state machine, deadlock-prone reader park/condvar, ack accounting). Sequential per-milestone TDD delegation (RED→GREEN→REFACTOR) per Anthropic's "most coding tasks involve fewer truly parallelizable tasks than research." Implementation Kickoff Approval obtained (사용자 승인: 착수 + Phase 1 재실행).
- cycle_type: tdd (quality.yaml constitution.development_mode: tdd)
- Route: A (Hybrid Trunk main-direct, Tier M default; no PR; manager-develop commits+pushes per B9)
- Implementation note (from audit): §A.9 requires refactoring `output.rs:49-60` — move `last_emitted_seq` load/store inside the ring-lock scope so replay() collect+store and pump_once collect+store serialize via the same lock (R16). Tauri event emit stays outside the lock. Verified by AC-15.
