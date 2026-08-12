# SPEC-PTY-FLOW-001 — 수용 기준 (acceptance)

## §A 개요

결함 1(프론트엔드 방향 흐름 제어 부재)과 결함 2(전환 시 출력 유실)의 수정을 관찰 가능한 증거로 판정한다. 모든 판정은 명령 실행 + verbatim 출력 기반(검증 주장 무결성 — 실행하지 않은 검증을 PASS로 보고하지 않는다). autotest 판독은 리포트 파일이 정본이며, autotest는 terminal-f 팬 밖에서 실행한다.

---

## §B Given-When-Then 시나리오

### 시나리오 1 — 활성 팬 홍수에서 프리즈 없음 (결함 1)

- **Given**: 활성 워크스페이스의 팬에서 대량 출력 프로그램(pwsh 루프로 지속적 벌크 텍스트 방출)이 실행 중이고,
- **When**: 출력 속도가 그 팬의 파싱+렌더 처리량을 지속적으로 초과하면,
- **Then**: (a) 백엔드 emitter는 `outstanding > FLOW_HIGH_WATERMARK`에서 해당 세션 방출을 중지하고, (b) ring이 `RING_PAUSE_THRESHOLD`를 넘으면 reader가 park하여 자식 프로세스의 write()가 블로킹되며, (c) ack 진전이 관측되고 `outstanding`은 `FLOW_HIGH_WATERMARK + 1배치` 이내로 유지되며, (d) 활성 팬에 "[output overflow]" 배너가 나타나지 않고, (e) 출력 종료 후 최종 꼬리(tail)가 정상 렌더된다.

### 시나리오 2 — 홍수 중 워크스페이스 전환에도 내용 공백 없음 (결함 2)

- **Given**: 팬에 홍수 출력이 진행 중이고 미파싱 백로그(write 큐/IME 보류)가 존재하며,
- **When**: 사용자가 워크스페이스를 이탈(snapshotAndDispose)했다가 스트림 진행 중 복귀(remount + `replay_pane(parsedSeq)`)하면,
- **Then**: 스냅샷 경계와 replay 경계 사이에 내용 공백(gap)이 없다 — 스냅샷은 파싱 완료분까지, replay는 `parsedSeq` 이후를 이어붙이며, 중복 없이 연속된 출력이 렌더된다. 또한 이탈 시 flow 회계가 리셋되어(R15) 이탈 직전 `outstanding > FLOW_HIGH_WATERMARK`였더라도 remount + replay 재무장 후 live 방출이 즉시 재개된다.

### 시나리오 3 — 죽은 프론트엔드에서 자식 프로세스 미웨지 (정지 밸브)

- **Given**: reader가 park 상태이고,
- **When**: `FLOW_STALL_TIMEOUT`(10s) 동안 ack 진전이 전혀 없으면,
- **Then**: reader는 읽기를 재개하고 oldest-drop + 기존 overflow 배너 경로로 폴백하며, 자식 프로세스는 write 블로킹에서 풀려난다.

### 시나리오 4 — park 중 teardown 즉시 join

- **Given**: reader가 park 상태이고,
- **When**: `teardown_session`이 호출되면,
- **Then**: condvar signal로 reader 스레드가 지체 없이(park 재확인 주기 이내) join된다.

---

## §C 엣지 케이스 검증 항목

| 케이스 | 검증 방법 |
|---|---|
| ack 초과(`acked > emitted`) | Rust 단위 테스트 — saturating_sub로 outstanding=0, 패닉 없음 |
| 미지 pane에 대한 늦은 ack | Rust 단위 테스트 — 조용히 무시, Err 아님 |
| 히스테리시스 경계(LOW < outstanding < HIGH) | Rust 단위 테스트 — 직전 상태 유지(진동 없음) |
| park 중 `replay_synced=false` 전이 | Rust 단위 테스트 — 즉시 disarm, 비활성 의미론 적용 |
| park 중 injection `require_idle` | Rust 단위 테스트 — BUSY 판정 |
| IME 보류 데이터 | 코드 경로 검토 + 수동 검증(§D.3) — 보류 중 미ack, flush의 write 콜백에서만 ack (호출 지점 구조 검증은 AC-10c) |
| 드레인 타임아웃 | 시나리오 2 autotest가 커버(타임아웃 시에도 replay가 공백을 채움) |
| 전환 시 좌초 outstanding (`> HIGH`) | Rust 단위 테스트 — 전이/재무장 시 회계 리셋, remount 후 live 방출 재개 (AC-14) |
| replay()–pump_once 동시 실행 | Rust 단위 테스트 — seq 되감김·replay 구간 중복 재방출 없음 (AC-15) |

---

## §D AC 매트릭스

각 AC는 관찰 가능해야 한다(테스트 출력, 리포트 파일, grep 결과). AC sub-ID 규약: 짝 기준은 소문자 접미(a/b)로 표기.

| AC | 요구사항 | 기준 | 검증 명령/방법 | 심각도 |
|---|---|---|---|---|
| AC-1 | R1, R3 | 워터마크 게이트 순수 로직 단위 테스트 green — skip(>HIGH)·resume(<=LOW)·히스테리시스 유지 | `cargo test` (신규 flow 게이트 테스트) | MUST |
| AC-2 | R1, R2, R13 | ack 회계 단위 테스트 green — u64 누적, saturating 방어, 미지 세션 무시 | `cargo test` | MUST |
| AC-3 | R4, R5 | reader 게이트 park/unpark 조건 단위 테스트 green — live+임계 초과 시 park, disarm 조건 즉시 해제 | `cargo test` | MUST |
| AC-4 | R6 | 정지 밸브 단위 테스트 green — 무ack 10s 후 재개+폴백, ack 진전 시 타이머 리셋 | `cargo test` (타임아웃 축소 주입 가능해야 함) | MUST |
| AC-5 | R8 | teardown-under-park join 단위 테스트 green — **ring이 `RING_PAUSE_THRESHOLD` 초과를 유지한 상태에서** disarm 플래그 설정 → condvar signal 순서로 reader join이 완료된다(깨어난 reader의 재park 없음) | `cargo test` | MUST |
| AC-6 | R7 | flow-paused 세션 BUSY 판정 단위 테스트 green | `cargo test` | MUST |
| AC-7 | R14 | 기존 Rust 테스트 스위트 전부 green (baseline ~92건, 실패 0) | `cd src-tauri && cargo test` | MUST |
| AC-8 | R14 | 기존 autotest 32 체크 전부 green | `TERMF_AUTOTEST=1` 실행(팬 밖) → 리포트 파일 판독 | MUST |
| AC-9 | R1, R3, R9 | 신규 autotest **flood** 체크 green — ack 진전, `outstanding <= HIGH + 1배치`, 활성 팬 overflow 배너 없음, 최종 tail 정상 (시나리오 1). `outstanding`/`emitter_paused` 관측은 `flow_stats(pane_id)` 디버그 커맨드 폴링으로 수행한다(백엔드 원자 변수의 유일한 프론트 관측 창구 — R1) | autotest가 `flow_stats` 폴링 결과를 리포트 필드로 기록 → 리포트 판독 | MUST |
| AC-10a | R10, R11 | 신규 autotest **switch-under-load** 체크 green — 전환 경계 내용 공백 없음 (시나리오 2, 결함 2 회귀) | autotest 리포트 판독 | MUST |
| AC-10b | R10 | `main.ts` 이벤트 핸들러가 `lastSeq`(정본 seq)를 직접 전진시키지 않음 — replay/snapshot 경로는 `parsedSeq`만 사용 | 코드 grep + 리뷰 (`grep -n "lastSeq\|parsedSeq" src/main.ts src/terms.ts`) | MUST |
| AC-10c | R12 | `src/terms.ts`의 `ackOutput` 호출 지점이 `term.write(data, cb)` 콜백 경로 **안에만** 존재하고, `imeBuffering` 보류 경로(버퍼 적재 지점)에는 존재하지 않음 | 코드 grep + 호출 지점 전수 리뷰 (`grep -n "ackOutput" src/terms.ts`) — 실제 IME 동작 검증은 §D.3 잔여 위험으로 유지 | MUST |
| AC-11 | R4, R14 | bench soak 시나리오 — reader park 발생·해제 관측, 활성 세션 ring 무한 성장 없음, ack 흐름 중 oldest-drop 없음 | `cargo run --bin bench` (soak) 출력 — bench가 ack 진행을 합성하고 워터마크/타임아웃을 주입 (plan §B.5) | SHOULD |
| AC-12 | 문서화 의무 | `docs/ADR-014-*.md` 신규(한국어) + `docs/DEVELOPMENT.md`·`docs/ARCHITECTURE.md` 갱신 | 파일 존재 + 내용 검토 (sync 단계) | MUST (sync) |
| AC-13 | @MX | reader 게이트·정지 밸브 `@MX:WARN`(+REASON), 워터마크 상수·회계 불변식 `@MX:ANCHOR` 부착(설명 ko) | `grep -n "@MX:" src-tauri/src/{session,output}.rs src/terms.ts` | SHOULD |
| AC-14 | R15 | flow 회계 리셋 단위 테스트 green — (i) `replay_synced=false` 전이, (ii) `replay()` 재무장, (iii) 정지 밸브 발화 각각에서 `outstanding == 0`. 핵심 케이스: **`outstanding > FLOW_HIGH_WATERMARK` 상태에서 워크스페이스 전환 → remount + replay 재무장 후 emitter가 live 방출을 재개**한다(영구 정지 없음) | `cargo test` | MUST |
| AC-15 | R16 | replay()–pump_once 경합 단위 테스트 green — 교차 실행에서 `last_emitted_seq` 되감김 없음, replay된 구간의 live 중복 재방출 없음 | `cargo test` | MUST |

### §D.1 품질 게이트

- TDD 사이클 준수: 각 마일스톤 RED 커밋 이력 존재.
- 신규 Rust 순수 로직 커버리지 목표 85% (커밋당 최소 80%).
- `cargo clippy` NEW 경고 0 (pre-flight baseline 대비).
- 프론트엔드는 vanilla TS로 단위 테스트 프레임워크가 없다 — autotest(AC-9/10)가 프론트 기계 검증의 정본이다.

### §D.2 Definition of Done

1. AC-1 ~ AC-10c, AC-14, AC-15 (MUST) 전부 PASS — verbatim 증거 포함.
2. AC-11, AC-13 (SHOULD) PASS 또는 사유 기록된 명시적 연기.
3. AC-12는 sync 단계 완료 조건.
4. 자기 검증 보고가 plan.md §E 형식(Claim/Evidence/Baseline/Gaps/Residual-risk)을 따름.

### §D.3 잔여 위험 (Residual Risk)

- **실제 IME는 autotest로 검증 불가** (저장소 공지 한계): 한국어 IME 조합 중 Claude Code 스트리밍 홍수 시나리오는 실기기 수동 검증으로만 확인한다. 수동 검증 결과는 run 단계 완료 보고의 Gaps/Residual-risk 항에 기록한다.
- **워터마크 seed 값의 환경 의존성**: 128KiB/32KiB는 개발 기기 계측 기반 seed다. 저사양 기기에서의 최적값은 다를 수 있다 — 상수화되어 있으므로 후속 조정 비용은 낮다.
- **WebGL 유실 자체는 미해결**(범위 제외): 강등 상태의 낮은 처리량은 지속되며, 본 SPEC은 그 상태에서의 무한 백로그만 차단한다.
