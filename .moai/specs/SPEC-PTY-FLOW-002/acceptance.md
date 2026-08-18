# SPEC-PTY-FLOW-002 — 수용 기준 (acceptance)

## §A 개요

ack 회계 단위 불일치로 인한 emitter 영구 정지의 수정을 **관찰 가능한 증거**로 판정한다. 모든 판정은 명령 실행 + verbatim 출력 기반이다(검증 주장 무결성 — 실행하지 않은 검증을 PASS로 보고하지 않는다). autotest 판독은 리포트 파일이 정본이며, autotest는 terminal-f 팬 **밖에서** 실행한다.

본 SPEC은 재현-우선(TDD)이다. AC-2(RED)는 수정 **이전**에 확보해야 하는 증거이며, 사후에 되돌아가 만들어낼 수 없다.

---

## §B Given-When-Then 시나리오

### 시나리오 1 — 비ASCII 홍수에서 팬이 영구 정지하지 않는다 (핵심 결함)

- **Given**: 활성 워크스페이스의 팬에서 한국어·박스 드로잉 문자가 지배적인 대량 출력이 진행 중이고,
- **When**: 누적 방출 바이트가 `FLOW_LOW_WATERMARK`(32KiB)를 여러 배 넘어설 만큼 출력이 지속되면,
- **Then**: (a) `acked`가 지속적으로 전진하고, (b) `outstanding`이 상한 안에서 유지되며(무한 증가 없음), (c) emitter가 정지 상태로 고착되지 않고, (d) 출력 종료 후 최종 꼬리가 정상 렌더된다.

### 시나리오 2 — 결함 재현: UTF-16 단위 ack은 회복 불가능한 결손을 만든다

- **Given**: emit 회계가 UTF-8 바이트로 이루어지는 방출 경로가 있고,
- **When**: 비ASCII 페이로드에 대해 UTF-16 코드 유닛 수로 ack하면(수정 이전 프론트엔드 동작),
- **Then**: `outstanding`의 바닥값이 누적 결손만큼 남아 `FLOW_LOW_WATERMARK` 아래로 내려가지 못하고 emitter 게이트가 정지 상태로 고착된다. 동일 경로를 이벤트 `byteLen`으로 ack하면 `outstanding`이 `FLOW_LOW_WATERMARK` 이하로 내려가 방출이 재개된다.

### 시나리오 3 — emitter 정지 사각지대의 자가 치유 (안전밸브)

- **Given**: emitter가 워터마크 게이트에 의해 정지했고 ring 미방출 바이트는 `RING_PAUSE_THRESHOLD` 미만이라 reader가 park하지 않은 상태이며,
- **When**: `stall_timeout` 동안 ack 진전이 전혀 관측되지 않으면,
- **Then**: 밸브가 발화하여 회계를 리셋하고(`outstanding = 0`) 방출이 재개되며, 발화 사실이 `flow_stats`로 관측된다.

### 시나리오 4 — 정상적으로 느린 팬에서는 밸브가 발화하지 않는다

- **Given**: emitter가 정지 상태이고,
- **When**: `stall_timeout`을 넘겨 시간이 흐르는 동안 ack이 (느리게라도) 계속 전진하면,
- **Then**: 밸브 타이머가 매번 리셋되어 밸브는 발화하지 않으며, 미확인 구간이 회계상 삭제되지 않는다.

### 시나리오 5 — 기존 흐름 제어 동작 무회귀

- **Given**: SPEC-PTY-FLOW-001이 확립한 ASCII 홍수 흐름 제어와 전환-중-부하 연속성이 동작 중이고,
- **When**: 본 SPEC의 변경이 적용되면,
- **Then**: 기존 ASCII 홍수 체크와 전환-중-부하 체크가 그대로 통과하고, reader park 밸브·teardown disarm·회계 리셋 3지점·R16 상호 배제의 단위 테스트가 전부 green이다.

---

## §C 엣지 케이스 검증 항목

| 케이스 | 검증 방법 |
|---|---|
| 오버플로 배너가 붙은 이벤트 | Rust 단위 테스트 — `record_emit` 가산값과 이벤트 `byteLen`이 동일(배너 포함 최종 문자열 기준) |
| 이모지·서로게이트 페어 | Rust 단위 테스트 — 4바이트/2코드 유닛 문자 포함 페이로드에서 반사 ack 후 `outstanding = 0` |
| IME 보류 이벤트 2건 이상 일괄 flush | 프론트엔드 구조 검증(grep) + 코드 경로 검토 — `heldAckBytes`가 개별 `byteLen`의 합 |
| `seq` 없는 합성 배너 | AC-16 (b) — `meta` 인자 미전달로 `ackBytes = 0` 경로 |
| replay·스냅샷 데이터 | AC-16 (a)(c) — `writeParsedNoAck` 경유, `ackBytes = 0` 고정 |
| 미마운트 팬으로 향한 이벤트 (알려진 누수 a) | 수정 범위 밖 — §D.3 등재 + R8 밸브 흡수. 검증 대상 아님 |
| ack IPC 실패 (알려진 누수 b) | 수정 범위 밖 — §D.3 등재 + R8 밸브 흡수. 검증 대상 아님 |
| 밸브 발화 직후 옛 ack 배치 도착 | Rust 단위 테스트 — `saturating_sub`로 `outstanding = 0` 바닥, 패닉 없음 |
| 두 밸브(reader/emitter) 동시 조건 | Rust 단위 테스트 — 회계 리셋은 멱등(`acked := emitted`), 이중 발화에도 상태 정합 |
| 정지 중 워크스페이스 전환 | Rust 단위 테스트 — R15 리셋이 밸브 도달 전에 `outstanding = 0` 달성(기존 AC-14 회귀 가드) |

---

## §D AC 매트릭스

각 AC는 관찰 가능해야 한다(테스트 출력, 리포트 파일, grep 결과).

**AC-ID 네임스페이스 규약**: 본 표의 `AC-N`은 **SPEC-PTY-FLOW-002 고유 번호**다. 선행 SPEC의 수용 기준을 가리킬 때는 언제나 `FLOW-001 AC-N` 형태로 접두 표기하며, 접두가 없는 `AC-N`은 본 SPEC의 것이다(선행 SPEC과 번호가 겹치되 의미가 다른 AC-9 / AC-10a에서 오독을 막기 위한 규약).

**AC sub-ID 규약**: 하나의 논리 AC를 이루는 짝 기준은 소문자 접미(a/b/c…)로 표기한다. Tier M 수용 기준 상한(16건) 대비 본 SPEC의 논리 AC는 **16건**(AC-1 ~ AC-16)이며, AC-10의 6개 sub-ID는 1건으로 계수한다.

| AC | 요구사항 | 시나리오 | 기준 | 검증 명령/방법 | 심각도 |
|---|---|---|---|---|---|
| AC-1 | R1, R2 | — | `PtyOutputEvent`가 `byteLen`을 실어 나르고, emit 회계 가산값과 이벤트 `byteLen`이 동일 최종 문자열에서 산출된다(배너 포함 케이스 포함) | `cargo test` (신규 이벤트 회계 테스트) + `grep -n "byte_len" src-tauri/src/output.rs` + `grep -n "byteLen" src/types.ts` | MUST |
| AC-2 | R12 | 2 | **RED 증거** — 수정 이전, 비ASCII 페이로드를 `pump_once`로 방출하고 이벤트 payload의 UTF-16 코드 유닛 수로 ack하는 배선 수준 재현 테스트가 실패하고 그 실패 출력이 verbatim으로 기록된다 | `cd src-tauri && cargo test <repro_test>` (수정 **전** 실행) → 실패 출력 캡처 | MUST |
| AC-3 | R12, R3 | 2 | **GREEN** — 동일 배선 경로를 이벤트 `byteLen`으로 ack하면 `outstanding <= FLOW_LOW_WATERMARK`에 도달하고 emitter 게이트가 방출 재개를 반환한다 | `cd src-tauri && cargo test <repro_test>` (수정 **후**) → green | MUST |
| AC-4 | R3, R5 | 1 | 프론트엔드 ack 수치의 출처가 이벤트 `byteLen`이며 `data.length` 유래 값이 아니다. `grep -n "data.length" src/terms.ts`의 매치는 **정확히 1건**(`view.outBufLen += data.length` — IME 버퍼 용량 캡 계산, ack과 무관한 정당 용법)만 남고, `ackBytes` / `heldAckBytes` 대입식 어디에도 `data.length`가 나타나지 않는다. 현행 매치 4건(`:199` 주석 / `:200` ack 산출 / `:210` `outBufLen` / `:212` 보류 누적) 중 `:200`·`:212`는 `byteLen`으로 교체되고 **`:199` 주석은 M1에서 문구를 함께 고쳐야** 이 기준이 성립한다(주석이 `data.length`를 언급한 채 남으면 매치가 2건이 되어 올바른 코드에서 AC가 실패한다) | `grep -n "data.length" src/terms.ts` → 매치 수 정확히 1, 그 1건이 `outBufLen` 행 · `grep -n "ackBytes\|heldAckBytes" src/terms.ts` → 모든 대입식의 우변이 `byteLen` 유래 | MUST |
| AC-5 | R4 | 1 | IME 보류 버퍼가 개별 이벤트 `byteLen`의 **합**을 누적하며, flush 시 이어붙인 문자열 길이를 재산정하지 않는다 | `grep -n "heldAckBytes" src/terms.ts` → 가산 대상이 `byteLen`, `TextEncoder` 매치 0건 | MUST |
| AC-6 | R7, R8 | 3 | 축소 `FlowConfig` 주입 하에서 emitter 정지 + ack 무진전 시 `stall_timeout` 경과 후 밸브가 발화하고, 회계가 리셋되며(`outstanding = 0`) 게이트가 방출 재개를 반환한다. 정지 진입 tick(무장 tick)에는 발화하지 않는다 | `cargo test` (신규 emitter 밸브 테스트) | MUST |
| AC-7 | R9 | 4 | emitter 정지 중 ack이 전진하는 동안에는 `stall_timeout`을 넘겨도 밸브가 발화하지 않는다(`emitter_valve_fired` 0 유지) | `cargo test` (밸브 부정 테스트) | MUST |
| AC-8 | R10 | 3 | `flow_stats(pane_id)` 응답에 `valveFired`와 `emitterValveFired` 두 필드가 **신규 노출**되고 emitter 밸브 발화가 그 값으로 관측된다. 기존 5필드(`emitted` / `acked` / `outstanding` / `emitterPaused` / `readerParked`)의 이름·타입·의미·값은 불변이다 | `cargo test` (FlowStats 직렬화 테스트) + `grep -n "emitter_valve_fired\|valve_fired" src-tauri/src/flow_state.rs` + `grep -n "emitterValveFired\|valveFired" src/types.ts` → Rust·TS 양쪽 필드 존재 | MUST |
| AC-9 | R11 | 5 | 기존 Rust 테스트 스위트가 전부 통과하고 신규 실패가 0건이다 | `cd src-tauri && cargo test` → 기존 135 + 신규 green | MUST |
| AC-10a | R13 | 1 | 비ASCII 홍수 autotest에서 `acked`가 전진한다 | autotest 리포트 `checks.u8FloodAckProgress === true` | MUST |
| AC-10b | R13 | 1 | 비ASCII 홍수 autotest에서 `outstanding` 최대값이 512KiB 이하로 유지된다 | autotest 리포트 `checks.u8FloodOutstandingBounded === true` | MUST |
| AC-10c | R13 | 1 | 비ASCII 홍수 종료 후 꼬리 마커가 렌더된다 | autotest 리포트 `checks.u8FloodTailRendered === true` | MUST |
| AC-10d | R13 | 1 | 비ASCII 홍수 최종 표본에서 emitter가 정지 상태로 고착되지 않는다 | autotest 리포트 `checks.u8FloodNoPermanentPause === true` | MUST |
| AC-10e | R13 | 1 | **반사 ack 균형 수치 판정** — 비ASCII 홍수 최종 표본의 `acked / emitted >= 0.9`. UTF-16 회귀 시 이 비율이 약 0.35로 떨어지므로 결함을 수치로 잡는다 | autotest 리포트 `checks.u8FloodAckRatio === true` | MUST |
| AC-10f | R13 | 1 | **밸브 구제 배제** — 비ASCII 홍수 구간 동안 `emitterValveFired` 증가량이 0이다. 균형이 밸브의 10초 주기 회계 리셋이 아니라 정상 ack으로 달성되었음을 보장한다 | autotest 리포트 `checks.u8FloodNoValveRescue === true` | MUST |
| AC-11 | R11 | 5 | 기존 ASCII 홍수 체크(`FLOW-001 AC-9`)와 전환-중-부하 체크(`FLOW-001 AC-10a`)가 그대로 통과하고, 흐름 제어 집계와 일반 집계가 **둘 다** green이다 | autotest 리포트에서 `checks.floodAckProgress` / `floodOutstandingBounded` / `floodNoOverflow` / `floodTailRendered` / `switchUnderLoadNoGap` 전부 `true` **AND** `flowOk === true` **AND** `ok === true` (두 필드 병기 — `ok` 체인은 흐름 제어 체크를 구조적으로 배제하므로 `ok`만으로는 흐름 제어를 전혀 판정하지 못한다) | MUST |
| AC-12 | R1, R3 | — | 타입 검사와 린트가 통과하고 신규 경고가 없다. `writeOutput` 호출부에서 `seq`만 넘기고 `byteLen`을 빠뜨리는 형태가 **컴파일 오류**가 된다(§A.2 객체 묶음 시그니처) | `npx tsc --noEmit` → exit 0 · `cd src-tauri && cargo clippy --all-targets` → NEW 경고 0 | MUST |
| AC-13 | R11 | 5 | bench 흐름 경로의 **기능 무회귀** — `flow_ok=true`. bench 실행 시 환경변수 `TERMF_FLOW_STALL_TIMEOUT_MS=60000`으로 `stall_timeout`을 확대하여 Phase A의 10초 무ack 구간에서 emitter 밸브가 발화하지 않으며(B10 완화책 — spawned 세션은 `FlowState::default()`로 고정되어 config 주입 경로가 없으므로 env 오버라이드가 유일한 수단), 리포트 표본의 `emitter_valve_fired`가 0이다 | `cd src-tauri && TERMF_FLOW_STALL_TIMEOUT_MS=60000 cargo run --bin bench` → `flow_ok=true` AND 전 표본 `emitter_valve_fired == 0` | SHOULD |
| AC-14 | §C @MX | — | emitter 밸브에 `@MX:WARN`(+`@MX:REASON`), 반사 ack 계약에 `@MX:ANCHOR`가 한국어로 부착된다 | `grep -n "@MX:" src-tauri/src/flow_state.rs src/terms.ts` | SHOULD |
| AC-15 | §C 문서화 | — | ADR-014가 회계 단위(UTF-8 바이트, 백엔드 단일 원천 + 반사 ack)와 emitter 밸브를 반영하여 개정되고, `ARCHITECTURE.md` §6 / `DEVELOPMENT.md` / `CHANGELOG.md`가 갱신된다 | 파일 diff 확인 (sync 단계 책임) | MUST (sync) |
| AC-16 | R6 | 1, 2 | **replay·스냅샷·합성 배너 미ack 무회귀** — `writeOutput` 시그니처 변경 후에도 (a) `main.ts`의 모든 `writeParsedNoAck` 호출부가 `ackBytes = 0` 경로를 경유하고, (b) 합성 배너 `writeOutput` 호출부가 `meta` 인자를 전달하지 않으며, (c) `writeParsedNoAck` 정의가 `writeParsed(view, data, seq, 0)`를 유지한다 | (a) `grep -c "terms.writeParsedNoAck(" src/main.ts` → **5** · (b) `grep -n "terms.writeOutput(" src/main.ts` → 실 PTY 호출 2건만 `meta` 전달, 배너 호출은 2인자 · (c) `grep -n -A 3 "export function writeParsedNoAck" src/terms.ts` → 반환식이 `writeParsed(view, data, seq, 0)` | MUST |

#### AC-16 판정 근거 (grep 패턴 정밀도)

**(a) 패턴은 `terms.writeParsedNoAck(` — 접두 한정이 필수다.** 무한정 `grep -c "writeParsedNoAck" src/main.ts`는 **6**을 반환한다: 실제 호출 5건에 더해 `src/main.ts:316`의 **주석** 한 줄(`// 완료 시 전진 → writeParsedNoAck 콜백이 replay.lastSeq 로 갱신…`)이 함께 매치되기 때문이다. 즉 무한정 패턴으로 "5건"을 기대하면 **올바른 코드에서 AC가 실패한다**. `terms.` 접두 + `(` 를 붙여 호출부만 세면 정확히 5건이 된다.

호출부 5건(전량 열거 — run 단계가 대조할 정본):

| # | 위치 | 용도 |
|---|---|---|
| 1 | `src/main.ts:300` | 스냅샷 시각 복원 |
| 2 | `src/main.ts:309` | 오버플로 배너 |
| 3 | `src/main.ts:317` | replay 데이터 |
| 4 | `src/main.ts:324` | `[process exited]` 배너 |
| 5 | `src/main.ts:328` | `[no session]` 배너 |

**(b) 판정 범위는 `src/main.ts`로 한정한다.** `src/autotest.ts:607`과 `src/autotest.ts:623`에도 2인자 `terms.writeOutput(imePane, imeMarker)` 형태의 호출이 있으나, 이들은 autotest가 IME 버퍼링 동작을 검사하려고 **합성 청크를 주입**하는 것이라 `seq`도 `byteLen`도 없는 것이 정상이다(ack 대상이 아니므로 R6 위반이 아니다). AC-16의 grep은 `src/main.ts`만 대상으로 하므로 이 두 건은 판정에 포함되지 않는다 — 시그니처를 `meta?` 객체로 바꿔도 2인자 호출은 그대로 유효하다.

---

### §D.1 품질 게이트

| 게이트 | 기준 |
|---|---|
| Tested | 신규/변경 Rust 순수 로직 단위 테스트 필수. 목표 커버리지 85%, 커밋당 최소 80%. 재현 테스트는 배선 수준(R12) |
| Readable | 코드 주석 한국어(`code_comments: ko`). §A.6 배너 회계 주석 정정 포함 |
| Unified | `cargo fmt` / 기존 TS 스타일 준수. 신규 상수 도입 없음 |
| Secured | 외부 입력 표면 변경 없음. `saturating_sub` 방어 유지(패닉·래핑 없음) |
| Trackable | Conventional Commits (`fix(SPEC-PTY-FLOW-002): M{N} ...`), 마일스톤별 커밋 SHA를 progress.md에 기록 |

### §D.2 Definition of Done

- [ ] AC-1 ~ AC-12(AC-10a~f 포함) 및 AC-16 전부 PASS (MUST)
- [ ] AC-2 RED 실패 출력이 progress.md §E.2에 verbatim으로 기록됨
- [ ] AC-13, AC-14 PASS 또는 미충족 사유가 명시됨 (SHOULD)
- [ ] AC-15 sync 단계 완료 (ADR-014 개정 + 동반 문서)
- [ ] `cargo test` / `cargo clippy` / `tsc --noEmit` 전부 green, NEW 경고 0
- [ ] autotest 리포트에서 **`ok === true` AND `flowOk === true` 두 필드 병기 확인** (실기기 실행, 팬 밖에서). `ok`만 확인하는 것은 흐름 제어를 전혀 판정하지 않은 것과 같다 — `ok`의 32체크 AND 체인은 흐름 제어 체크를 구조적으로 배제한다
- [ ] plan.md §D PRESERVE 목록 무변경 확인 (`git diff --stat`으로 스코프 검증). `report.flowOk` 집계식의 append-only 확장과 `FlowStats` 필드 추가는 명시된 예외이므로 위반이 아니다

### §D.3 잔여 위험 (Residual Risk)

- **TS 배선에 대한 테스트 러너 부재 (구조적 갭 — 최우선 잔여 위험)**: 결함이 실제로 사는 배선은 프론트엔드(`src/terms.ts`의 ack 수치 산출)인데, 이 프로젝트에는 TS 테스트 러너가 없다(`package.json` scripts = `dev / build / preview / tauri`, vitest·jest 설정 및 `*.test.ts` 0건). spec.md §C 의존성 제약이 신규 npm 의존성 도입을 금지하므로 러너 도입은 현행 제약 아래 선택지가 아니다. 따라서 **배선 수준 Rust 재현 테스트(AC-2/AC-3)는 백엔드 emit 회계 경로만 덮으며, `terms.ts`가 나중에 `data.length`로 되돌아가도 그 Rust 테스트는 계속 green이다**.
  - TS 배선의 기계 가드는 정확히 두 축이다: **(i) 구조 grep** — AC-4(매치 수 정확히 1건 + `ackBytes`/`heldAckBytes` 대입식 우변 검증), AC-5, AC-16. 이들은 값이 아니라 코드 구조를 고정하는 저비용 트립와이어다. **(ii) 실기기 autotest** — AC-10a~f, 특히 AC-10e(`acked/emitted >= 0.9`)가 UTF-16 회귀를 수치로 잡는 유일한 종단 판정이며 AC-10f가 그 판정이 밸브 구제로 통과하는 경로를 차단한다.
  - 두 축 모두 컴파일 시점이 아니라 리뷰·실행 시점 가드이므로, TS 러너 도입은 후속 SPEC 후보로 남긴다.
- **실기기 IME 조합 중 비ASCII 홍수**: autotest는 합성 이벤트 기반이라 실제 IME 조합 상태에서의 보류-합산 경로(R4)를 완전히 재현하지 못한다. 수동 검증 항목으로 남긴다(한국어 입력 조합 중 Claude Code 출력 홍수).
- **PowerShell 콘솔 인코딩 환경 의존**: 비ASCII autotest 페이로드가 UTF-8로 출력되지 않는 환경에서는 체크가 결함이 아니라 인코딩을 측정할 수 있다. 체크 실패 시 인코딩 확인이 선행 절차다.
- **알려진 잔여 회계 누수 2건(수정 범위 밖)**: (a) `writeOutput`이 미마운트 팬에서 조기 반환하여 이미 emit된 바이트가 영구 미ack로 남는 경로, (b) `ackOutput` invoke 실패를 삼켜 해당 배치 ack이 유실되는 경로. 두 경로 모두 본 SPEC에서 수정하지 않으며 emitter 밸브(R8)가 흡수한다. spec.md §D와 plan.md §B(B11/B12)에 등재했으므로 "미지의 누수"가 아니다 — 실사용에서 밸브 발화가 관측되면 1순위 조사 대상이다.
- **성능 계측 미수행**: spec.md §C 성능 제약은 `byteLen`이 이미 소유한 `String`에서 O(1)로 산출될 것을 요구할 뿐 정량 계측을 요구하지 않는다. AC-13은 bench의 **기능 무회귀**(`flow_ok=true`)만 판정하며 처리량·지연 수치는 비교하지 않는다(bench의 `flow_ok`는 기능 플래그이지 성능 지표가 아니다). 성능 회귀 여부는 미계측 상태로 남으며, 필드 추가가 emit 경로에 순회·복사·인코딩을 도입하지 않는다는 코드 구조 검토가 근거다.
- **밸브 `stall_timeout` 10초 동안의 체감 지연**: 밸브 경로로 빠지는 경우 최대 10초 출력 지연이 발생한다. 이는 열화 경로의 의도된 비용이며, 1차 해법(단위 통일)이 정상 동작하는 한 도달하지 않는다.
