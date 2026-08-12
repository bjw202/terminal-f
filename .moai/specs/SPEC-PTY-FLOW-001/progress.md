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

### M2 — 프론트엔드 ack/parsedSeq/드레인 (완료 2026-08-12, 커밋 5d26a52)

- 위임: manager-develop 에이전트가 1-pass 완료(429 없음 — 할당량 해소). 오키스트레이터 독립 검증 완료.
- `terms.ts`: writeOutput→writeParsed(data,cb) 전환, ack 배치(R9: ACK_BATCH_BYTES 4KiB / ACK_FLUSH_IDLE_MS 50ms), appendOutput(IME 보류)은 heldAckBytes 누적만(R12), flushAckNow(ackInFlight 추적), snapshotAndDispose 드레인(Promise.race + SNAPSHOT_DRAIN_TIMEOUT_MS 500ms, R11), serialize 직전 flushAckNow await(R15 late-ack).
- `main.ts`: receivedSeq/parsedSeq 분리(R10), pty-output 핸들러는 receivedSeq만 전진(AC-10b), replayInFlight+pendingReplayEvents로 replay 중 이벤트 지연(R10-N2), writeParsedNoAck로 replay 미ack(R13), switchTo가 snapshotAndDispose await(R15).
- `ipc.ts`: ackOutput 래퍼(R2).
- tsc --noEmit: **exit 0** (에이전트 + 오키스트레이터 양쪽 독립 확인 — node `~/bin/node`).
- AC-10b grep PASS: 핸들러(main.ts:809) receivedSeq만; parsedSeq는 writeParsed cb(terms.ts:252)·mountPane 설정만.
- AC-10c grep PASS: ackOutput 유일 호출(terms.ts:313 flushAckNow); appendOutput(207-217)에 ack 호출 없음(R12).
- @MX: terms.ts 7개(ANCHOR 2 + WARN 1 + NOTE 4).
- Gaps: autotest 동작 검증(AC-9/10a)·bench soak(AC-11)은 M3 소관. 실기기 IME는 §D.3 잔여위험.
- Residual-risk: 전환 직전 ack invoke 백엔드 도달 순서는 Tauri 커맨드 큐 FIFO 가정에 의존(프로젝트 단일 스레드 순차 처리로 성립). 워터마크 seed는 개발기기 계측 기반.

### R4 reader-park 게이트 단위 수정 (버그 픽스, 커밋 1c6c55d)

- 결함: `check_reader_park_gate` 가 미방출 **청크 수**(seq 차이, 최대 ~1024)를 `ring_pause_threshold`(**바이트**, 768KiB)와 비교 → reader park 가 실제로 발동 안 함(R4 MUST 무력화 — 활성 팬 홍수 시 자식 write() 블로킹이 안 돼 oldest-drop 유실 경로로 폴백). M3 bench(311샘플 전부 reader_parked=false)로 발견됨.
- 수정: `RingBuffer::un_emitted_bytes(from_seq)` 추가, `check_reader_park_gate` + 신규 비블로킹 관측 helper `reader_should_park_now` 를 바이트 기준으로 전환.
- RED→GREEN: 재현 테스트 `ac_3_reader_park_gate_uses_bytes_not_chunks`(청크 1개 + 임계 초과 바이트 → true) — RED 확정 후 수정으로 GREEN.
- 종단 검증: bench 재실행 시 `reader_parked false→true` 전이 관측(outstandingEndA ~814KiB > 768KiB), `flow_ok=true`, `oldest_drop_during_ack=0`.
- 135 tests green, clippy NEW 0. (gap 1 — `spawn_session`의 FlowConfig 주입 경로는 별개 개선과제; default config로 reader park 관측 가능했으므로 본 수정에 불필요.)

## §E.3 Run-phase Audit-Ready Signal

run_status: audit-ready (모든 MUST AC 확정)
run_complete_at: 2026-08-12
run_commit_sha: 1c6c55d
autotest_verified_at: 2026-08-12 (실기기, report `ok:true`)

run-phase commits: `3c01b46`(M1 RED) → `9cc45c8`(M1 GREEN) → `5d26a52`(M2) → `0439cf7`(M3) → `1c6c55d`(R4 fix).

AC status:
- **MUST PASS** (단위/구조/bench): AC-1, AC-2, AC-3, AC-4, AC-5, AC-6, AC-7(`cargo test` 135 green), AC-10b, AC-10c, AC-14, AC-15 — 전부 PASS.
- **MUST PASS** (실기기 autotest, 사용자 실행 2026-08-12): AC-8(기존 체크 + 신규 flow 체크 전부 green, `ok:true`), AC-9 flood(`ackProgress:true`, `maxOutstanding:32665` ≪ HIGH, `noOverflowBanner:true`, `tailRendered:true`), AC-10a switch-under-load(`progressed:true` 67→419행, `noGap:true` expected 419 = got 419 — 결함 2 회귀 확정 차단). 리포트: `src-tauri/autotest-report.json`. switch p95=85.3ms (<150).
- **SHOULD**: AC-11(bench soak) PASS(`flow_ok=true`, reader_parked 관측 — R4 fix 후), AC-13(@MX) 부착.
- **sync**: AC-12(ADR-014) — sync 단계 책임.

Gaps: 실기기 IME + Claude Code 스트리밍 홍수 수동 검증(§D.3 — 합성이벤트 한계). gap 1(FlowConfig 주입 경로) — 별개 개선. (flood 시 `emitterPaused/readerParked`는 실기기 처리량이 충분해 발동 안 함 — 정상; bench 극단 부하에서 발동 관측됨.)

## §E.4 Sync-phase Audit-Ready Signal

sync_status: audit-ready
sync_complete_at: 2026-08-12
sync_commit_sha: 9b177eeb7d1b0e55fca4b63c128fec171b30def1

Sync-phase artifacts delivered:
- ADR-014 created (`docs/ADR-014-pty-flow-control.md`, 한국어) — 배경(결함 1, 결함 2) → 결정(ack-watermark, reader park, 정지 밸브, parsedSeq 이원화) → 워터마크 근거(seed 값 + bench/autotest 계측) → 트레이드오프(vs oldest-drop 단독, vs VS Code 문자 기반) → 테스트 구조(Rust 단위 테스트 + autotest flood/switch + bench soak) → R4 fix 비고
- `docs/DEVELOPMENT.md` 갱신 — ADR-014 추가(ADR-001~014) + 백엔드 모듈 지도(flow_state.rs 추가, output.rs/session.rs 흐름 제어 반영)
- `docs/ARCHITECTURE.md` 갱신 — §6 출력 흐름에 ack-watermark 게이트, reader park, 정지 밸브, parsedSeq 이원화, 회계 리셋 서술 추가
- `CHANGELOG.md` 생성 — [Unreleased] 섹션에 SPEC-PTY-FLOW-001 엔트리 작성
- README.md/GUIDE-features-easy.md 무변경 — §A.7 사용자 가시 변화는 존재(아래 목록)하나, README 개요·GUIDE 기능 설명의 기존 서술과 충돌하지 않고 사용자 개입이 필요 없는 동작 개선이므로 별도 기술 서술은 불필요. 상세는 CHANGELOG §Changed + ADR-014에 기록.
- spec.md frontmatter `status: in-progress` → `status: completed` 전이 완료 (updated: 2026-08-12)
- @MX 태그 검증 완료 — 총 **25개** (`flow_state.rs`=13[워터마크/reader-park/밸브·outstanding 불변식 ANCHOR·WARN 핵심], `terms.ts`=7, `autotest.ts`=2, `bench.rs`=3; session.rs/output.rs=0). (참고: 9b177ee 커밋 메시지의 "총 7개"는 `flow_state.rs` 누락 + 산술 오류 — 본 라인에서 정정.)
- AC-12(문서화) PASS: ADR-014 + 동반 문서 완료

User-visible changes (§A.7 검토):
- 활성 팬 홍수 시 프리즈 대신 자식 프로세스 자연 감속 → 사용자 가시 변화
- 워크스페이스 전환 시 내용 공백 제거 → 사용자 가시 변화  
- "[output overflow]" 배너가 정지 밸브 폴백 경로에서만 나타남 → 활성 팬 정상 경로에서 제거(사용자 가시 변화)


## §F Phase 4 Mode Selection

- tier: M | scope: ~7 files (session.rs/output.rs/commands.rs/lib.rs + terms.ts/main.ts/ipc.ts + autotest/bench) | domains: 4 (Rust backend, TS frontend, autotest, bench/docs) | concurrency benefit: LOW (coding-heavy, new concurrency logic)
- Mode 1 trivial: no | Mode 2 background: no (write work) | Mode 3 agent-team: RETIRED | Mode 4 parallel: no (coding-heavy → Mode 5 per Anthropic coding-task parallelism caveat) | Mode 6 workflow: no (semantic new logic + inter-file deps, not high-volume mechanical) | Mode 5 sub-agent: **SELECTED**
- Decision: `sub-agent` (sequential, per-milestone M1→M2→M3)
- Justification: Tier M coding-heavy SPEC with inter-file dependencies and concurrency-sensitive new logic (flow-control state machine, deadlock-prone reader park/condvar, ack accounting). Sequential per-milestone TDD delegation (RED→GREEN→REFACTOR) per Anthropic's "most coding tasks involve fewer truly parallelizable tasks than research." Implementation Kickoff Approval obtained (사용자 승인: 착수 + Phase 1 재실행).
- cycle_type: tdd (quality.yaml constitution.development_mode: tdd)
- Route: A (Hybrid Trunk main-direct, Tier M default; no PR; manager-develop commits+pushes per B9)
- Implementation note (from audit): §A.9 requires refactoring `output.rs:49-60` — move `last_emitted_seq` load/store inside the ring-lock scope so replay() collect+store and pump_once collect+store serialize via the same lock (R16). Tauri event emit stays outside the lock. Verified by AC-15.
