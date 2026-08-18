# SPEC-PTY-FLOW-002 — 근본 원인 분석 (research)

조사 시점: 2026-08-18 · 대상 트리: `main` 브랜치 작업 트리 · 선행 SPEC: SPEC-PTY-FLOW-001 (`status: completed`)

본 문서는 코드베이스 직접 조사로 확인한 사실만 기록한다. 각 주장은 `file:line` 증거를 동반하며, 추론과 관측을 구분하여 서술한다.

---

## §A 증상 (사용자 보고, 재현 가능)

- Claude Code처럼 비ASCII TUI 출력(박스 드로잉, 한국어, 이모지)이 많은 프로그램을 실행하는 팬에서 출력이 **영구히 멈춘다**.
- 동시에 입력 에코가 사라져 입력이 죽은 것처럼 보인다.
- 워크스페이스를 다른 곳으로 전환했다가 돌아오면 밀려 있던 출력이 한꺼번에 나타나고 정상으로 돌아온다.
- 같은 팬에서 곧 다시 재발한다.
- SPEC-PTY-FLOW-001(흐름 제어) 도입 **이후**에 나타나기 시작했다.

---

## §B 증거표 (file:line)

| # | 사실 | 증거 |
|---|---|---|
| E1 | 백엔드 emit 회계는 **UTF-8 바이트**를 센다 | `src-tauri/src/output.rs:92` — `session.flow_state.record_emit(data.len());` (Rust `String::len()` = 바이트 길이) |
| E2 | 프론트엔드 ack 회계는 **UTF-16 코드 유닛**을 센다 | `src/terms.ts:200` — `writeParsed(view, data, seq, seq !== undefined ? data.length : 0);` (JS `string.length` = 코드 유닛) |
| E3 | IME 보류 경로도 동일 결함 | `src/terms.ts:212` — `view.heldAckBytes += data.length;` |
| E4 | ack 누적치는 `writeParsed` 콜백에서 배치로 흘러 들어간다 | `src/terms.ts:244` `writeParsed(...)` → `src/terms.ts:254` `view.ackPendingBytes += ackBytes;` → `src/terms.ts:288` `ACK_BATCH_BYTES` 도달 시 플러시 → `src/terms.ts:302` `flushAckNow` |
| E5 | `outstanding = emitted - acked` (saturating) | `src-tauri/src/flow_state.rs:130-134` — `em.saturating_sub(ak) as usize` |
| E6 | emitter 게이트는 히스테리시스를 갖는다 | `src-tauri/src/flow_state.rs:171-182` — 정지 중이면 `outstanding <= low_watermark`에서만 재개, 방출 중이면 `outstanding <= high_watermark`에서만 유지 |
| E7 | 워터마크 값 | `src-tauri/src/flow_state.rs:20-21` — `FLOW_HIGH_WATERMARK = 128 KiB`, `FLOW_LOW_WATERMARK = 32 KiB` |
| E8 | R6 정지 안전밸브는 **reader park 루프 안에만** 존재한다 | `src-tauri/src/flow_state.rs:203-244` `check_park_and_wait` 루프 내부에서만 `fire_valve_internal` 호출; 발화 본체는 `src-tauri/src/flow_state.rs:275-282` |
| E9 | reader는 ring 미방출 바이트 > `RING_PAUSE_THRESHOLD`(768KiB) **이고** `replay_synced`일 때만 park한다 | `src-tauri/src/session.rs:936-945` — `un_emitted_bytes` + `replay_synced` → `check_park_and_wait`; 임계값은 `flow_state.rs:29` |
| E10 | 워크스페이스 이탈 시 회계 리셋 | `src-tauri/src/session.rs:803-807` — `replay_synced.store(false)` + `flow_state.reset_accounting()` (R15(i)) |
| E11 | 복귀(replay) 시 회계 재무장 | `src-tauri/src/session.rs:684-686` — `replay_synced.store(true)` + `flow_state.reset_accounting()` (R15(ii)) |
| E12 | 프론트엔드는 replay 데이터를 미ack로 쓴다 | `src/main.ts:300`, `src/main.ts:309`, `src/main.ts:317` — `terms.writeParsedNoAck(...)`; 미ack 보장은 `src/terms.ts:275-282` (`ackBytes = 0` 고정) |
| E13 | 기존 홍수 autotest 페이로드는 순수 ASCII | `src/autotest.ts:273` — `"$pad='F'*200; 1..6000 | ForEach-Object { \"FLOODL $_ $pad\" }; 'TERMF_FLOOD_DONE'\r"` |
| E14 | 이벤트 구조체에는 바이트 길이 필드가 없다 | `src-tauri/src/output.rs:31-38` — `workspace_id / pane_id / session_id / seq / data`만 존재 |
| E15 | 프론트엔드 이벤트 타입도 동일 | `src/types.ts:229-235` — `workspaceId / paneId / sessionId / seq / data` |
| E16 | 배너 회계에 대한 주석과 코드가 어긋난다 | `src-tauri/src/output.rs:90-92` — 주석은 "배너 접두어 길이는 흐름 회계에서 제외"라고 하나, `record_emit(data.len())`은 배너가 붙은 `data`에 대해 호출된다 |
| E17 | **알려진 잔여 누수 (a)** — 미마운트 팬으로 향한 이벤트의 바이트가 영구 미ack로 남는다 | `src/terms.ts:193` — `const view = views.get(paneId); if (!view) return;` 조기 반환. 백엔드는 이미 `record_emit`으로 emitted를 전진시킨 뒤이므로 그 구간은 ack될 기회가 없다(팬 마운트 이전/직후 창) |
| E18 | **알려진 잔여 누수 (b)** — ack IPC 실패 시 해당 배치의 ack이 영구 유실된다 | `src/terms.ts:313` — `ipc.ackOutput(view.paneId, bytes).catch((e) => console.warn("[ack]", e))`. 예외를 삼키고 재시도하지 않으므로 `ackPendingBytes`는 이미 0으로 비워진 뒤 유실된다 |
| E19 | `FlowStats`는 밸브 발화 카운터를 노출하지 않는다 | Rust `src-tauri/src/flow_state.rs:316-324` — 5필드(`emitted`/`acked`/`outstanding`/`emitter_paused`/`reader_parked`). TS `src/types.ts:125-131` 동일. `valve_fired`는 내부 카운터(`flow_state.rs:95`)이며 노출 경로는 `valve_fired_count()`(`flow_state.rs:284`) 하나, 호출자는 테스트 2곳뿐 |
| E20 | autotest의 `report.ok`는 흐름 제어 체크를 **구조적으로 배제**한다 | `src/autotest.ts:786-815` — 32개 체크를 명시 열거하며 흐름 체크 미포함. `src/autotest.ts:817-822` 주석이 그 배제를 의도로 명시. 흐름 집계는 별도 `report.flowOk`(`src/autotest.ts:824-831`, 5항 AND) |
| E21 | TS 테스트 러너가 프로젝트에 없다 | `package.json:6-11` — scripts = `dev` / `build` / `preview` / `tauri`. vitest·jest 설정 파일 및 `*.test.ts` 0건 |
| E22 | bench Phase A는 정확히 10초 무ack 구간이며 축소 config를 주입하지 않는다 | `src-tauri/src/bin/bench.rs:307-329` — `phase_a_deadline = phase_a_start + Duration::from_secs(10)`, 루프 내 ack 합성 없음. 기본 `stall_timeout`은 10초(`flow_state.rs:31`) — emitter 밸브 도입 시 경계 충돌 |
| E23 | **bench는 `FlowConfig`를 주입할 수 없다** (E22 완화책의 반증 — `with_config` 경로는 실행 불가) | registry가 만드는 세션은 `FlowState::default()`로 하드코딩되어 있다: `src-tauri/src/session.rs:522`, `src-tauri/src/session.rs:1217`. `FlowState::with_config`는 standalone 인스턴스 전용이며 spawned 세션은 `Arc<PtySession>` 배후라 생성 후 config 필드 수정도 불가하다. bench 자신이 이 불가능성을 주석으로 기록해 두었다 — `src-tauri/src/bin/bench.rs:263-271` ("M1 이 spawn_session 에 config 주입 경로를 제공하지 않았다 … spawned 세션은 Arc<PtySession> 배후라 config 필드 수정 불가"). 따라서 완화책은 `FlowConfig::default()`의 env 오버라이드여야 한다 |
| E24 | bench의 표본 JSON은 `FlowStats` 직렬화가 아니라 **손수 조립한 필드 나열**이다 | `src-tauri/src/bin/bench.rs:318-327`(Phase A), `src-tauri/src/bin/bench.rs:358-366`(Phase B) — 6키를 명시 나열. Rust `FlowStats`에 필드를 추가해도 이 두 블록에는 자동으로 반영되지 않는다 |
| E25 | `main.ts`의 무한정 `writeParsedNoAck` grep은 주석 1건을 함께 센다 | `grep -c "writeParsedNoAck" src/main.ts` → **6** (호출 5 + `src/main.ts:316` 주석 1). `grep -c "terms.writeParsedNoAck(" src/main.ts` → **5** (호출부만) |
| E26 | `terms.ts:199` 주석이 `data.length`를 언급한다 | `src/terms.ts:199` — `// ackBytes: seq 가 있는 배치(실 PTY 출력)만 data.length 만큼 ack 누적.` 현행 `data.length` 매치 4건 중 하나이며, 코드만 고치고 주석을 두면 매치가 2건으로 남는다 |

---

## §C 결함 메커니즘 (단계별)

### C.1 결손의 발생

한국어 음절(`한`)과 박스 드로잉 문자(`─`)는 UTF-8에서 **3바이트**, UTF-16에서 **1코드 유닛**이다. 이모지는 UTF-8 4바이트 / UTF-16 2코드 유닛이다.

- 백엔드는 이벤트 방출 시 `emitted += 3` (E1)
- 프론트엔드는 파싱 완료 시 `acked += 1` (E2)

문자 하나마다 **2바이트가 영구히 부족**하게 보고된다. `outstanding = emitted - acked` (E5)는 실제 미확인량이 아니라 "실제 미확인량 + 누적 결손"을 가리킨다.

관측: 비ASCII 지배 출력에서 결손은 방출 바이트의 약 2/3에 달한다. 32KiB(`FLOW_LOW_WATERMARK`)의 결손은 약 48KiB의 비ASCII 출력만으로 도달한다.

### C.2 결손이 정지를 영구화하는 경로

E6의 히스테리시스에 따르면:

1. `outstanding`이 `FLOW_HIGH_WATERMARK`(128KiB)를 넘으면 emitter가 정지한다.
2. 정지 상태에서는 `outstanding <= FLOW_LOW_WATERMARK`(32KiB)일 때만 재개한다.
3. 그런데 누적 결손만으로 `outstanding`의 **바닥값**이 이미 32KiB를 넘어섰다면, 프론트엔드가 남은 데이터를 모두 파싱하고 ack해도 `outstanding`은 32KiB 아래로 내려가지 않는다.
4. 방출이 없으므로 새 ack도 발생하지 않는다 → `outstanding` 불변 → **재개 조건 영구 미달**.

결과: emitter 영구 정지. 팬은 프리즈된 것처럼 보인다.

입력이 죽어 보이는 이유: `write_pane`은 흐름 제어를 거치지 않으므로 입력은 PTY에 정상 도달한다. 그러나 자식 프로세스가 되돌려주는 **에코가 emitter를 통과하지 못해** 화면에 나타나지 않는다.

### C.3 자가 치유 사각지대

E8/E9가 결정적이다. 기존 R6 정지 안전밸브는 `check_park_and_wait`의 park 루프 **안에서만** 발화하고, reader는 ring 미방출 바이트가 768KiB를 넘어야 park한다.

따라서 다음 구간에는 어떤 자가 치유 경로도 없다:

> 누적 결손 > 32KiB **이면서** ring 미방출 바이트 < 768KiB

Claude Code의 TUI 출력은 화면 갱신 위주라 순간 총량이 크지 않으므로 대부분 이 사각지대에 들어간다. (출력이 계속 쏟아져 768KiB를 넘기면 reader가 park하고 10초 후 R6 밸브가 발화한다 — **그 경로는 정상 동작하며 회귀시켜서는 안 된다**.)

### C.4 워크스페이스 전환이 복구시키는 이유

E10 → E11 → E12의 연쇄다.

1. 이탈: `replay_synced = false` + `reset_accounting()` → `acked := emitted` → `outstanding = 0` (E10)
2. 복귀: `replay_pane`이 ring을 수집하고 다시 `reset_accounting()`으로 재무장 (E11)
3. 프론트엔드는 replay 데이터를 `writeParsedNoAck`로 써서 ack하지 않는다 (E12) — byte 회계를 오염시키지 않는다
4. `outstanding = 0` → emitter 재개 → 밀려 있던 출력이 쏟아진다

이 복구 동작 자체는 설계대로이며 **보존 대상**이다. 사용자가 관측한 "전환하면 낫는다"는 결함의 증상이 아니라 R15 리셋이 제대로 동작한다는 증거다.

### C.5 왜 기존 autotest가 놓쳤는가 (두 겹의 이유)

**첫째 — 페이로드가 ASCII다.** E13이 답이다. FLOW-001 AC-9 홍수 체크의 페이로드는 `'F'*200` 패딩 + ASCII 라벨로만 구성되어 있다. **ASCII 구간에서는 UTF-8 바이트 수와 UTF-16 코드 유닛 수가 일치**하므로 두 회계가 우연히 어긋나지 않는다. 결함은 비ASCII 페이로드에서만 드러난다.

**둘째 — 집계가 분리되어 있다.** E20이 답이다. 설령 흐름 체크가 결함을 잡았더라도, `report.ok`의 32체크 AND 체인은 흐름 제어 체크를 **의도적으로 배제**한다(그 배제 자체는 합리적 설계다 — 흐름 체크 실패가 기존 32체크 판정을 가리지 않게 한다). 따라서 `ok:true`만 보고 "autotest 통과"로 판정하면 흐름 제어는 전혀 측정되지 않는다. 본 SPEC의 판정 기준이 `report.flowOk`여야 하는 이유이며, 신규 비ASCII 체크도 그 집계식에 편입되어야 한다.

### C.6 잔여 누수 2경로 (수정 범위 밖, 등재 대상)

E17·E18의 두 경로는 `byteLen` 반사와 무관하게 회계 결손을 만들 수 있다.

- **(a) 미마운트 팬 조기 반환** (`terms.ts:193`): 백엔드가 `record_emit`을 마친 뒤 프론트엔드가 팬을 찾지 못해 반환하면, 그 바이트는 ack될 기회를 갖지 못한다. 워크스페이스 전환·팬 생성 직후의 짧은 창에서 발생한다.
- **(b) ack IPC 실패 삼킴** (`terms.ts:313`): `ackPendingBytes`를 0으로 비운 **뒤** invoke하므로, invoke가 거부되면 그 배치는 재시도 없이 유실된다.

두 경로 모두 본 SPEC에서 **수정하지 않는다**. 다만 §D.3이 이들을 "미지의 누수"로 뭉뚱그리지 않도록 spec.md §D 엣지 표와 plan.md §B(B11/B12)에 명시 등재한다 — 작업 트리에서 이미 식별 가능한 경로를 미지로 남길 이유가 없고, 실사용에서 밸브 발화가 관측되면 이 둘이 1순위 조사 대상이 된다.

---

## §D 선행 교훈과의 연속성

SPEC-PTY-FLOW-001의 R4 reader-park 게이트에도 같은 **단위 불일치** 결함이 있었다: `check_reader_park_gate`가 미방출 **청크 수**(최대 ~1024)를 `ring_pause_threshold`(**바이트**, 768KiB)와 비교하여 reader park가 실제로 발동하지 않았다. 그 결함은 M3 bench(311표본 전부 `reader_parked=false`)에서야 발견되었다 — 단위 테스트가 하위 함수를 직접 호출하며 배선을 우회했기 때문에 은폐되어 있었다.

본 SPEC의 결함은 **같은 클래스의 두 번째 사례**다. 따라서 검증 전략은 다음을 요구사항으로 고정한다(spec R12):

> 재현 테스트는 **실제 이벤트 페이로드 경로를 구동**해야 한다. `record_ack`를 이미 올바른 바이트 수로 직접 호출하는 단위 테스트는 배선(이벤트 → ack 수치 산출)을 우회하므로 이 결함 클래스를 구조적으로 은폐한다.

**다만 이 전략에는 제약이 만든 한계가 있다.** 이번 결함이 사는 배선은 프론트엔드(`terms.ts`)인데 이 프로젝트에는 TS 테스트 러너가 없고(E21) 신규 npm 의존성 도입도 금지되어 있다. 따라서 Rust 배선 테스트는 **백엔드 emit 회계 경로까지만** 덮으며, `terms.ts`가 나중에 `data.length`로 되돌아가도 그 테스트는 계속 green이다. R12는 이 한정을 명시적으로 서술하고, TS 배선의 기계 가드를 (i) 구조 grep(AC-4/AC-5/AC-16)과 (ii) 실기기 autotest 수치 판정(AC-10e/AC-10f) 두 축으로 지정한다. 이 갭은 acceptance.md §D.3의 최우선 잔여 위험으로 등재된다.

---

## §E 해법 후보 비교

| 후보 | 장점 | 단점 | 판정 |
|---|---|---|---|
| **A. 이벤트가 `byteLen`을 실어 나르고 프론트가 반사** | 산정 지점이 하나(백엔드). 결함 클래스 자체가 제거됨. O(1) 비용(이미 소유한 `String`) | 이벤트 스키마에 필드 1개 추가(내부 IPC 전용) | **채택** (spec R1~R5) |
| B. 프론트엔드가 `TextEncoder`로 자체 산정 | 백엔드 무변경 | 산정 지점이 둘 → 인코딩 경계에서 재발 여지. 매 청크 인코딩 비용 | 기각 (spec §E) |
| C. 백엔드가 UTF-16 코드 유닛으로 회계 전환 | 프론트엔드 무변경 | 매 emit마다 문자열 순회. ring 용량·워터마크가 모두 바이트 축이라 축 불일치 | 기각 |
| D. ack을 seq 기반으로 전환 | 단위 문제 자체가 사라짐 | "미확인 **바이트**"라는 배압 신호의 의미 상실. SPEC-PTY-FLOW-001 구조 전면 재개방 | 기각 |
| **E. emitter 정지 안전밸브 추가 (A와 병행)** | §C.3 사각지대 제거. 미지의 잔여 누수에 대한 최종 방어선 | 열화 경로(지연 + 배너 가능). 1차 해법이 아님 | **채택** (spec R7~R10, 방어 심화) |

채택 조합: **A(1차 정합성) + E(최종 방어선)**. E를 단독 채택하면 결함이 "10초마다 복구되는 느린 팬"으로 위장되어 근본 원인이 잠복한다.

---

## §F 영향 범위

| 파일 | 변경 성격 |
|---|---|
| `src-tauri/src/output.rs` | `PtyOutputEvent`에 `byte_len` 필드 추가, 최종 문자열에서 1회 산출, §B E16 주석 정정 |
| `src-tauri/src/flow_state.rs` | `FlowInner`에 정지 진전 추적 필드 2개(`paused_since` / `paused_at_acked`), `emitter_gate_decision`에 무장·발화 판정, `emitter_valve_fired` 카운터 + `FlowStats`에 `valve_fired` / `emitter_valve_fired` 두 필드 **신규 추가**(E19 — 현행 5필드에는 밸브 카운터가 없다) |
| `src-tauri/src/flow_tests.rs` | 배선 수준 재현 테스트(RED/GREEN 짝) + 밸브 발화/미발화 테스트 |
| `src/types.ts` | `PtyOutputEvent`에 `byteLen: number`(필수) + `FlowStats`에 `valveFired` / `emitterValveFired` 추가 |
| `src/terms.ts` | `writeOutput`을 `meta?: { seq; byteLen }` 형태로 전환, 200행·212행의 `data.length` 제거 |
| `src/autotest.ts` | 비ASCII 홍수 체크 6판정 신규 추가 + `report.flowOk` 집계식 append-only 확장(E20 — `report.ok`는 흐름 체크를 배제하므로 신규 체크를 그 체인에 넣지 않는다). 기존 체크 로직·판정 무변경 |
| `src-tauri/src/bin/bench.rs` | 손수 조립 표본 JSON 2곳(E24)에 `emitter_valve_fired` 추가 + `bench.rs:263-271` config-주입 불가 주석 갱신. **`stall_timeout` 확대 자체는 이 파일이 아니라 `flow_state.rs`의 `FlowConfig::default()` env 오버라이드로 구현한다** — bench는 `TERMF_FLOW_STALL_TIMEOUT_MS=60000`으로 실행될 뿐이다(E23이 `with_config` 주입 불가를 반증) |

무변경 확인 대상(PRESERVE): 워터마크 값, ack 배치 규율, reader park 구조, R15 리셋 3지점, R16 락 범위, `writeParsedNoAck` 경로, `report.ok` 32체크 체인, `FlowStats` 기존 5필드, control-pipe/automation/injection API 표면.

> 변경 파일이 6건에서 7건으로 늘었다(`bench.rs` 추가). 스코프 확장이 아니라 E22가 드러낸 필수 완화 조치이며, 변경 내용은 config 주입 1줄이다.

---

## §G 미검증 항목 (Gaps)

- 실기기에서 결함 발생 시점의 `flow_stats` 원시 표본은 수집하지 않았다. 근본 원인은 코드 경로 추적(§B, §C)으로 확정했으며, 실기기 수치 확인은 M2 autotest(AC-10)가 대신한다.
- PowerShell 콘솔 출력이 기본으로 UTF-8인지는 환경별로 확인이 필요하다(plan.md §A.5, B6).
- 결함 발생까지의 정확한 소요 출력량(문자 구성비에 따라 달라짐)은 계측하지 않았다. 판정에 필요하지 않다 — 결손이 단조 증가한다는 사실만으로 영구 정지가 성립한다(§C.2).
