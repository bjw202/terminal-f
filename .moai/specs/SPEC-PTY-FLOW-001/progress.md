# SPEC-PTY-FLOW-001 — progress

## §E.1 Plan-phase Audit-Ready Signal

plan_status: audit-ready
plan_complete_at: 2026-08-12
artifacts: spec.md (GEARS R1~R16) + plan.md (M1~M3, Tier M) + acceptance.md (AC-1~AC-15, sub-ID 포함 17항목)
plan_audit: iteration 1 findings D1~D10 반영 (spec 0.2.0)

## §E.2 Run-phase Evidence

Phase 1 Plan Audit Gate (re-run on v0.2.1, 2026-08-12): **PASS, 0.92** (Tier M threshold 0.80). v0.2.1 N1/N2/N3 assessed SOUND. 6 findings, all MINOR/NIT — non-blocking: D1 (R15:152 "영구 부풀린다" wording → "일시 과소 산정"), D2 (R15 frontend obligation buried — surfaced in M2 delegation as mitigation), D3 (R10 5-obligation density), D4 (AC-10d advisory for N2 replay-deferral — verification baked into M3), D5 (AC-4 phrasing NIT), D6 (R10 defer-buffer bound NIT). Spec-body wording fixes (D1/D2/D4 formal) deferred to sync-phase manager-spec touch. Report: `.moai/reports/plan-audit/SPEC-PTY-FLOW-001-2026-08-12.md`.

### M1 — 백엔드 flow 제어 (완료 2026-08-12)

- RED `3c01b46`: AC-1~AC-6, AC-14, AC-15 단위 테스트 + spec.md `draft → in-progress` 전이.
- GREEN `9cc45c8`: flow_state.rs(FlowState/FlowConfig/게이트/밸브/리셋/R16) + output.rs(pump_once 워터마크 게이트 + R16 collect/store 동일 ring 락 + R1 record_emit) + session.rs(reader park, R8 disarm 선행, R7 BUSY, R15 reset 3지점, 테스트 헬퍼) + commands.rs/lib.rs(ack_output, flow_stats).
- 위임 경위: manager-develop 에이전트가 GREEN 도중 API 사용량 한도(429, 18:54 리셋)로 중단. 오키스트레이터가 인라인 마무리 — 코드는 이미 컴파일되는 상태로 남아 있었고 잔여 작업은 2건 테스트 버그 수정(AC-1 반환 극성 반전, AC-14 reset 후 새 live 데이터 push 누락) + clippy 정리(items_after_test_module/useless_format)였음.
- cargo test: **134 passed / 0 failed** (Windows shell PATH 보정 시; Git Bash 최소 PATH에서는 `detect_shell_finds_something_on_windows` 1건 환경 실패 — 회귀 아님, 증거 `.moai/state/verify/pty-flow-m1/`).
- cargo clippy: NEW 경고 0. 잔여 경고(paste/spool/state/bench)는 사전 존재.
- AC PASS: AC-1, AC-2, AC-3, AC-4, AC-5, AC-6, AC-14, AC-15 (21개 flow 테스트 전부 green).
- Gaps: 커버리지 수치 별도 미측정(21개 단위 테스트로 핵심 순수 로직 커버). @MX 태그 부착(ANCHOR 워터마크·불변식 / WARN park·밸브, ko).
- Residual-risk: detect_shell 테스트는 shell이 PATH에 있어야 함(PowerShell에서 cargo 권장). M3 bench가 사용할 ack 합성·상수 주입은 FlowConfig로 이미 지원.

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
