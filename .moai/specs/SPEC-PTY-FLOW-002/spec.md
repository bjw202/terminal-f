---
id: SPEC-PTY-FLOW-002
title: "PTY 흐름 제어 ack 바이트 단위 불일치로 인한 출력 영구 정지 수정"
version: "0.1.0"
status: in-progress
created: 2026-08-18
updated: 2026-08-18
author: manager-spec
priority: P0
phase: "v0.1.3 target"
module: "src-tauri/src, src"
lifecycle: spec-anchored
tags: "pty, flow-control, ack, utf-8, byte-accounting, stall-valve, regression"
tier: M
depends_on: [SPEC-PTY-FLOW-001]
---

# SPEC-PTY-FLOW-002 — ack 바이트 단위 불일치 수정 + emitter 정지 안전밸브

## HISTORY

| 버전 | 날짜 | 작성자 | 변경 내용 |
|---|---|---|---|
| 0.1.0 | 2026-08-18 | manager-spec | 최초 작성 (plan-phase 아티팩트 생성). SPEC-PTY-FLOW-001 후속 결함 — 백엔드 UTF-8 바이트 회계와 프론트엔드 UTF-16 코드 유닛 회계의 단위 불일치로 인한 emitter 영구 정지. 해법: (D1) 이벤트 `byteLen` 반사 ack, (D2) emitter 측 정지 안전밸브, (D3) 재현-우선 테스트 의무 |

---

## §A 배경과 목적

### A.1 증상 (사용자 보고, 재현 가능)

Claude Code처럼 비ASCII TUI 출력(박스 드로잉 문자, 한국어, 이모지)이 많은 프로그램을 실행하는 팬에서, 일정 시간 후 출력이 **영구히 멈추고** 입력 에코도 사라진다. 워크스페이스를 다른 곳으로 전환했다가 돌아오면 밀려 있던 출력이 한꺼번에 나타나며 정상으로 돌아오지만, 같은 팬에서 곧 다시 재발한다. 이 증상은 SPEC-PTY-FLOW-001(흐름 제어) 도입 **이후에** 나타나기 시작했다.

### A.2 근본 원인 — 회계 단위 불일치

백엔드와 프론트엔드가 **서로 다른 단위**로 같은 회계 장부를 기록한다.

- 백엔드 emit 회계는 **UTF-8 바이트**를 센다: `src-tauri/src/output.rs:92`의 `session.flow_state.record_emit(data.len())` — Rust `String::len()`은 바이트 길이다.
- 프론트엔드 ack 회계는 **UTF-16 코드 유닛**을 센다: `src/terms.ts:200`의 `writeParsed(view, data, seq, seq !== undefined ? data.length : 0)` — JavaScript `string.length`는 코드 유닛 수다. IME 보류 경로(`src/terms.ts:212` `view.heldAckBytes += data.length`)도 동일한 결함을 갖는다.

한국어 음절과 박스 드로잉 문자는 UTF-8에서 3바이트지만 UTF-16에서는 1코드 유닛이다. 그런 문자 하나마다 ack이 2바이트씩 **부족하게** 보고된다. `outstanding = emitted - acked`(`flow_state.rs:130`)의 이 결손은 **영구적**이다 — 아무리 ack해도 메워지지 않고 누적된다.

emitter 게이트는 히스테리시스를 갖는다(`flow_state.rs:171`): `outstanding > FLOW_HIGH_WATERMARK`(128KiB)에서 정지하고 `outstanding <= FLOW_LOW_WATERMARK`(32KiB)에서만 재개한다. 누적 결손만으로 32KiB를 넘어서는 순간, 실제로 미확인 데이터가 없더라도 `outstanding`은 결코 LOW 아래로 내려갈 수 없다 → **emitter 영구 정지** = 팬 프리즈. 입력은 PTY에 정상 도달하지만(`write_pane`은 흐름 제어와 무관) 그 에코가 방출되지 않아 입력이 죽은 것처럼 보인다.

### A.3 자가 치유 사각지대

SPEC-PTY-FLOW-001의 R6 정지 안전밸브(10초 무진전 시 회계 리셋)는 **reader park 루프 안에만** 존재한다(`flow_state.rs:203`의 `check_park_and_wait`, 발화는 `flow_state.rs:275` `fire_valve_internal`). reader는 ring의 미방출 바이트가 `RING_PAUSE_THRESHOLD`(768KiB)를 넘고 `replay_synced`일 때만 park한다(`session.rs:936-945`).

따라서 사각지대가 존재한다: **결손 > 32KiB 이면서 ring 미방출 바이트가 768KiB 미만**인 구간에는 어떤 자가 치유 경로도 없다. Claude Code의 TUI 출력은 화면 갱신 위주라 총량이 크지 않으므로 대부분 이 사각지대에 들어간다. (출력이 계속 쏟아져 768KiB를 넘으면 reader가 park하고 10초 후 R6 밸브가 발화한다 — 그 경로는 정상 동작하며 회귀시키지 않는다.)

### A.4 워크스페이스 전환이 복구시키는 이유

전환 이탈 시 `session.rs:803-807`이 `replay_synced = false` + `reset_accounting()`(R15(i), `acked := emitted`)을 수행하고, 복귀 시 `replay_pane`이 `session.rs:684-686`에서 다시 `reset_accounting()`(R15(ii))으로 재무장한다. 프론트엔드 `mountPane`(`src/main.ts:290-330`)은 replay 데이터를 `writeParsedNoAck`로 쓴다(R13 — 미ack). 결과적으로 `outstanding = 0`이 되어 emitter가 재개되고 밀린 출력이 쏟아진다. 이 복구 동작 자체는 올바르며 **보존 대상**이다.

### A.5 기존 autotest가 놓친 이유

AC-9 홍수 체크의 페이로드는 순수 ASCII다(`src/autotest.ts:273` — `"$pad='F'*200; 1..6000 | ForEach-Object { \"FLOODL $_ $pad\" }"`). ASCII 구간에서는 UTF-8 바이트 수와 UTF-16 코드 유닛 수가 **우연히 일치**하므로 두 회계가 어긋나지 않는다. 결함은 비ASCII 페이로드에서만 드러난다.

선행 교훈과의 연속성: SPEC-PTY-FLOW-001의 R4 reader-park 게이트도 "청크 수 vs 바이트" 단위 불일치 결함이었고, 단위 테스트가 배선을 우회해 하위 함수를 직접 호출한 탓에 은폐되었다. 본 SPEC은 같은 은폐를 반복하지 않기 위해 **실제 이벤트 페이로드 경로를 구동하는 배선 수준 테스트**를 요구사항으로 고정한다(R12).

### A.6 채택된 해법 방향 (사용자 승인, 최종)

1. **단위 통일 — 백엔드 공급 바이트 길이 반사(reflect-ack)**: `PtyOutputEvent`가 자신의 UTF-8 바이트 길이를 실어 나르고, 프론트엔드는 그 값을 **그대로 되돌려** ack한다. 회계 단위의 단일 진실 원천은 백엔드다.
2. **사각지대 제거 — emitter 측 정지 안전밸브**: emitter가 정지한 채 ack 진전 없이 `FLOW_STALL_TIMEOUT`이 지나면 회계를 리셋하고 방출을 재개한다. 이는 **열화 경로**(지연 + ring 오버플로 배너 가능)이지 1차 정합성 수단이 아니다.

---

## §B 요구사항 (GEARS)

GEARS 구조 키워드(`Where` / `While` / `When` / `shall`)와 코드 식별자는 영문 정본을 유지하고, 서술부는 한국어로 기술한다.

### 명명 상수 (기존 상수 재사용 — 신규 임계값 없음)

| 상수 | 값 | 출처 | 본 SPEC에서의 역할 |
|---|---|---|---|
| `FLOW_HIGH_WATERMARK` | 128 KiB | `src-tauri/src/flow_state.rs` (기존) | 변경 없음 |
| `FLOW_LOW_WATERMARK` | 32 KiB | `src-tauri/src/flow_state.rs` (기존) | 변경 없음 |
| `FLOW_STALL_TIMEOUT` / `FlowConfig.stall_timeout` | 10 s | `src-tauri/src/flow_state.rs` (기존) | emitter 밸브가 **재사용**한다(신규 상수 도입 금지) |

### R1 — 이벤트가 자신의 바이트 길이를 실어 나른다 (Ubiquitous)

> The `PtyOutputEvent` **shall** 자신의 `data` 필드에 대한 UTF-8 바이트 길이를 `byteLen` 필드로 함께 전달한다(백엔드 직렬화 camelCase 규약 준수, 프론트엔드 타입 정의에도 동일 필드 존재).

`byteLen`은 이벤트에 실제로 실리는 **최종 문자열**의 바이트 길이다. 프론트엔드가 이 값을 신뢰할 수 있어야 반사 ack이 성립한다.

### R2 — emit 회계와 `byteLen`은 동일 원천 (Ubiquitous)

> The emitter **shall** 한 이벤트에 대해 emit 회계에 가산하는 바이트 수와 그 이벤트의 `byteLen` 값을 **동일한 최종 문자열**로부터 산출한다.

오버플로 배너 접두어가 붙은 경우에도 두 값은 동일하다(배너 포함 회계로 자기 정합). 현행 `output.rs:90-92` 주석은 "배너 접두어 길이는 흐름 회계에서 제외"라고 서술하나 코드는 배너가 붙은 문자열에 대해 `record_emit(data.len())`을 호출하고 있어 서술과 코드가 어긋나 있다 — 본 요구사항은 코드 쪽(배너 포함, 자기 정합)을 정본으로 확정하고 주석을 그에 맞춘다.

### R3 — 프론트엔드 ack은 `byteLen`을 반사한다 (Event-driven)

> **When** `pty-output` 이벤트의 `term.write` 콜백이 실행되면(xterm 파싱 완료), the frontend **shall** 그 이벤트의 `byteLen` 값을 ack 누적치에 가산한다 — 문자열 길이(`data.length`)를 ack 바이트로 사용하지 않는다.

SPEC-PTY-FLOW-001 R9의 배치 규율(`ACK_BATCH_BYTES` 4KiB 도달 또는 `ACK_FLUSH_IDLE_MS` 50ms idle 후 플러시)은 그대로 유지되며, 배치에 담기는 **수치의 단위만** 바뀐다.

### R4 — IME 보류 버퍼는 `byteLen`을 합산한다 (State-driven)

> **While** 팬이 IME 보류 상태(`imeBuffering`)이면, the frontend **shall** 보류된 각 이벤트의 `byteLen`을 **합산**하여 보류 ack 누적치를 유지하고, 플러시 시 그 합계를 ack 바이트로 사용한다.

한 이벤트 = 하나의 `data` 문자열 = 하나의 `byteLen`이므로, 여러 이벤트가 하나의 `term.write`로 병합되는 보류 경로에서는 개별 `byteLen`의 합이 유일하게 올바른 값이다. `seq`가 없는 합성 배너 청크는 emit 회계를 거치지 않았으므로 합산에서 제외한다.

### R5 — 프론트엔드는 바이트 길이를 재산정하지 않는다 (Unwanted)

> The frontend **shall not** 자체적으로 UTF-8 바이트 길이를 계산하여 ack에 사용한다.

(금지 규정 — 부정은 `shall not`에만 있다.) `TextEncoder` 등으로 프론트엔드가 독립 산정하면 인코딩 경계 사례에서 백엔드와 다시 어긋날 여지가 생긴다. 회계 단위의 단일 진실 원천은 백엔드이며, 프론트엔드의 역할은 **반사**뿐이다.

### R6 — replay·스냅샷·합성 배너 경로는 여전히 미ack (Unwanted)

> The frontend **shall not** replay 응답 데이터, 스냅샷 복원 데이터, `seq`가 없는 합성 배너를 ack한다.

(금지 규정 — 부정은 `shall not`에만 있다.) SPEC-PTY-FLOW-001 R13의 불변식을 그대로 유지한다. 이 경로들은 emit 회계를 거치지 않으므로 ack하면 `acked > emitted` 왜곡이 발생한다.

### R7 — emitter 정지 구간의 진전 관측 (State-driven)

> **While** emitter가 워터마크 게이트에 의해 정지 상태이면, the session flow-control state **shall** 정지 시작 시각과 정지 시작 시점의 누적 ack 값을 유지하여 "ack 진전 없음" 여부를 판정 가능하게 한다.

### R8 — emitter 정지 안전밸브 (Event-driven)

> **When** emitter가 정지한 채 `FlowConfig.stall_timeout` 동안 ack 진전이 전혀 관측되지 않으면, the session flow-control state **shall** 회계를 리셋하고(`acked := emitted`, 즉 `outstanding = 0`) 밸브 발화 횟수를 증가시킨 뒤 방출을 재개한다.

이 밸브는 SPEC-PTY-FLOW-001 R6(reader park 밸브)의 emitter 측 대응물이며, §A.3의 사각지대(결손 > LOW 이면서 ring 미방출 < `RING_PAUSE_THRESHOLD`)를 덮는다. **열화 경로**임을 명시한다 — 발화까지 `stall_timeout`만큼 출력이 지연되며, 그 사이 ring이 차면 오버플로 배너가 나타날 수 있다. 1차 정합성 수단은 R1~R5의 단위 통일이고, 본 밸브는 알려지지 않은 잔여 회계 누수에 대한 최종 방어선이다.

### R9 — ack 진전 시 밸브 타이머 리셋 (Event-driven)

> **When** emitter 정지 중 누적 ack 값이 전진하면, the session flow-control state **shall** 밸브 타이머를 리셋하여 밸브를 발화시키지 않는다.

정상적으로 ack이 흐르는(단지 느린) 팬에서 밸브가 오발화하여 미확인 구간을 회계상 삭제해서는 안 된다.

### R10 — 밸브 관측 가능성 (Ubiquitous)

> The session registry **shall** `flow_stats(pane_id)` 응답에 reader-park 밸브 발화 횟수(`valveFired`)와 emitter 밸브 발화 횟수(`emitterValveFired`)를 **신규 필드로 노출**하고, 기존 5필드(`emitted` / `acked` / `outstanding` / `emitterPaused` / `readerParked`)의 의미와 값은 변경하지 않는다.

현행 `FlowStats`는 위 5필드뿐이며 밸브 발화 카운터는 **노출되어 있지 않다** — `valve_fired` 는 내부 원자 카운터로만 존재하고 접근 경로가 테스트 전용 메서드 하나뿐이다. 따라서 본 요구사항은 "기존 필드 보존"이 아니라 **두 필드의 신규 노출**을 규정한다. 두 밸브를 하나의 카운터로 합치지 않는 이유는 R13의 autotest 판정이 "emitter 사각지대 밸브가 발화했는가"를 reader-park 밸브와 **구분해** 판정해야 하기 때문이다(합치면 그 구분이 사라진다).

### R11 — 기존 흐름 제어 계약 무회귀 (Ubiquitous)

> The app **shall** SPEC-PTY-FLOW-001의 R6(reader park 밸브), R8(teardown disarm 선행), R15(회계 리셋 3지점), R16(replay–emitter seq 상호 배제), 워크스페이스 전환 복구 동작을 변경 없이 유지하고, 기존 Rust 테스트 스위트와 기존 autotest 체크를 전부 통과한다.

### R12 — 재현-우선 배선 수준 회귀 테스트 (Ubiquitous)

> The regression suite **shall** **백엔드 emit 회계 경로**를 실제로 구동하여 결함을 재현하는 배선 수준 테스트를 포함한다 — 비ASCII(한국어·박스 드로잉) 페이로드를 `pump_once`로 방출하고 이벤트 payload에서 UTF-16 코드 유닛 수를 산출해 ack했을 때 `outstanding`의 바닥값이 누적 결손만큼 남아 emitter 정지가 해제되지 않음을 관측하며, 동일 경로를 이벤트의 `byteLen`으로 ack했을 때 `outstanding`이 `FLOW_LOW_WATERMARK` 이하로 내려가 방출이 재개됨을 관측한다.

`record_ack`를 이미 올바른 바이트 수로 직접 호출하는 단위 테스트는 본 요구사항을 충족하지 않는다 — 그런 테스트는 배선(이벤트 페이로드 → ack 수치 산출)을 우회하므로 단위 불일치를 구조적으로 은폐한다(§A.5).

**적용 범위 한정 (구조적 갭 명시)**: 본 요구사항은 **백엔드 경로에만** 적용된다. 결함이 실제로 사는 프론트엔드 배선(`src/terms.ts`의 ack 수치 산출)은 이 프로젝트에 TS 테스트 러너가 없고(§C 의존성 제약이 신규 npm 의존성 도입을 금지) 도입도 범위 밖이므로, 배선 수준 Rust 테스트만으로는 TS 쪽 회귀를 잡지 못한다. TS 배선의 기계 가드는 **R5·R6의 grep 판정 + R13의 실기기 autotest 두 축**이며, 이 갭은 제약이 만든 구조적 한계로서 acceptance.md §D.3에 잔여 위험으로 등재된다.

### R13 — 비ASCII 홍수 autotest 체크 (Ubiquitous)

> The autotest suite **shall** 한국어·박스 드로잉 문자가 지배적인 페이로드로 홍수 체크를 수행하여, (a) ack 진전, (b) `outstanding` 상한 유지, (c) 출력 꼬리 렌더, (d) 최종 표본에서 emitter가 정지 상태로 고착되지 않음, (e) 표본 최종 `acked / emitted` 비율이 0.9 이상, (f) 홍수 구간 동안 `emitterValveFired`가 0 유지를 기계 판정하고, 그 판정을 `report.flowOk` 집계에 편입한다.

판정 (e)와 (f)가 필수인 이유: (a)~(d)만으로는 R8 밸브가 `stall_timeout`마다 회계를 리셋하여 **결함이 있어도 전부 true**가 될 수 있다. (e)는 반사 ack이 실제로 균형을 맞추고 있음을 수치로 증명하고, (f)는 그 균형이 밸브 구제가 아니라 정상 ack으로 달성되었음을 보장한다.

**집계 대상 명시**: 판정 기준은 `report.ok`가 **아니라** `report.flowOk`다. 현행 `report.ok` 체인은 32개 체크를 명시 열거하며 흐름 제어 체크를 **의도적으로 배제**하고, 흐름 제어는 `report.flowOk`로 별도 집계된다. 따라서 신규 4~6 판정은 `report.flowOk` 집계식에 편입되어야 하며, **그 집계식 확장은 본 SPEC의 범위 안**이다(체크 로직 무변경 ≠ 집계식 무변경). 기존 ASCII 홍수 체크(FLOW-001 AC-9)와 전환-중-부하 체크(FLOW-001 AC-10a)의 **체크 로직과 판정 결과는 변경하지 않으며**, `flowOk` 집계식에서의 기존 5개 항은 그대로 보존한 채 신규 항만 AND로 추가한다.

---

## §C 제약 (Constraints)

| 구분 | 제약 |
|---|---|
| 아키텍처 불변식 | ADR-004 유지 — "느린 UI가 백엔드 메모리를 키울 수 없다". reader는 UI 완료를 기다리며 블로킹되지 않는다. 본 SPEC은 회계 **단위**만 정정하며 흐름 제어 구조는 변경하지 않는다. |
| 이벤트 스키마 | `PtyOutputEvent`에 필드 1개 추가는 내부 IPC 전용이다. 외부 공개 API·control-pipe 프로토콜·automation engine 표면은 변경하지 않는다. |
| autotest 집계 | `report.flowOk` 집계식 확장은 **범위 안**이다(R13). 기존 5개 항은 보존한 채 신규 항만 AND로 추가한다. `report.ok`의 32체크 체인은 **변경 금지** — 흐름 제어 체크를 그 체인에 편입하지 않는다(현행 분리 설계 유지). |
| 테스트 인프라 | TS 테스트 러너(vitest/jest 등) 도입은 하지 않는다(신규 npm 의존성 금지와 동일 축). 프론트엔드 배선 검증은 grep 구조 판정 + 실기기 autotest 두 축으로 한정한다(R12 적용 범위 한정). |
| 상수 규율 | 신규 임계값 상수 도입 금지. emitter 밸브는 기존 `FlowConfig.stall_timeout`을 재사용한다. |
| 테스트 주입 | 밸브·게이트 로직은 축소 `FlowConfig` 주입으로 결정론적 검증이 가능해야 한다(기존 `FlowConfig` 주입 패턴 준수). 실시간 10초 대기에 의존하는 테스트 금지. |
| 성능 | `byteLen`은 이미 소유한 `String`에서 O(1)로 산출되어야 한다. emit 경로에 추가 순회·복사·인코딩 비용을 도입하지 않는다. bench 홍수 경로에 유의미한 성능 회귀가 없어야 한다. |
| 의존성 | 신규 crate/npm 의존성 추가 없음. |
| 개발 방법론 | TDD (RED → GREEN → REFACTOR). `quality.yaml constitution.development_mode: tdd`. 재현 테스트가 RED의 정본이다. |
| 커버리지 | 신규/변경 Rust 순수 로직(밸브 판정, 정지 구간 진전 추적) 단위 테스트 필수. 목표 85%, 커밋당 최소 80%. |
| autotest 함정 | autotest(`TERMF_AUTOTEST=1`)는 앱을 자체 종료시킨다. **terminal-f 팬 안에서 절대 실행 금지**, 리포트 파일이 정본(scratchpad에 쓰고 밖에서 읽는다). |
| 문서화 의무 | `docs/DEVELOPMENT.md` §9 — 흐름 제어 설계 변경은 ADR-014(`docs/ADR-014-pty-flow-control.md`)의 정정·보강으로 기록한다(신규 ADR이 아니라 기존 ADR 개정 — 같은 결정의 단위 정정이므로). `docs/ARCHITECTURE.md` §6 서술도 갱신한다. 산출은 sync 단계 책임이며 검증 기준은 acceptance.md AC-15. |
| 문서 언어 | 모든 문서는 한국어(코드 식별자·파일명·명령어는 영어 유지). |
| @MX 태그 | emitter 밸브에 `@MX:WARN`(+`@MX:REASON`), `byteLen` 반사 ack 계약에 `@MX:ANCHOR` 후보. `code_comments: ko` — 태그 설명은 한국어. |

---

## §D 부록 — 엣지 케이스

| 케이스 | 기대 동작 |
|---|---|
| 한국어·박스 드로잉 지배 출력(Claude Code TUI) | 본 SPEC의 핵심 시나리오. `byteLen` 반사 ack으로 `outstanding`이 실제 미확인량을 정확히 반영 → 결손 누적 없음 → 영구 정지 없음 |
| 이모지·서로게이트 페어(UTF-16 2코드 유닛, UTF-8 4바이트) | 백엔드 산정값을 반사하므로 프론트엔드가 서로게이트 처리를 의식할 필요 없음(R5) |
| 오버플로 배너가 붙은 이벤트 | 배너 포함 최종 문자열로 emit 회계와 `byteLen`이 동일 산출(R2) — 자기 정합, ack 결손 없음 |
| IME 조합 중 여러 이벤트 보류 후 일괄 flush | 보류된 각 이벤트의 `byteLen` 합을 ack(R4). 개별 문자열을 이어붙인 뒤 길이를 재는 방식은 R5 위반이자 오답 |
| `seq` 없는 합성 배너(exit·no session 메시지) | emit 회계를 거치지 않으므로 ack 대상 아님(R6). 보류 합산에서도 제외 |
| replay·스냅샷 복원 데이터 | 미ack 유지(R6). `parsedSeq`는 전진(seq 회계와 byte 회계는 별개 축) |
| emitter 정지 + ring 미방출 < `RING_PAUSE_THRESHOLD` (사각지대) | R8 emitter 밸브가 `stall_timeout` 후 회계 리셋 + 재개. reader는 park하지 않았으므로 R6 reader 밸브는 관여하지 않음 |
| emitter 정지 + ring 미방출 > `RING_PAUSE_THRESHOLD` | 기존 R6 reader park 밸브 경로가 그대로 동작(무회귀, R11). 두 밸브가 모두 회계 리셋을 수행해도 리셋은 멱등(`acked := emitted`)이므로 안전 |
| 정지 중이지만 ack이 느리게나마 진전 | R9로 타이머 리셋 → 밸브 미발화. 정상적으로 느린 팬에서 미확인 구간이 회계상 삭제되지 않음 |
| 정지 중 워크스페이스 전환 | 기존 R15(i)/(ii) 리셋 경로가 먼저 `outstanding = 0`을 만들어 밸브 도달 전에 해소(무회귀) |
| 밸브 발화 직후 옛 ack 배치 도착 | `saturating_sub`로 `outstanding`은 0 바닥. 패닉·래핑 없음(기존 방어 유지) |
| 여러 팬 동시 홍수 | 회계·게이트·밸브 모두 세션별 독립. 한 팬의 밸브 발화가 다른 팬에 영향 없음 |
| **알려진 잔여 누수 (a)** — 마운트되지 않은 팬으로 향한 이벤트 | `writeOutput`이 `views.get(paneId)` 실패 시 조기 반환하므로, 이미 emit 회계에 가산된 바이트가 영구 미ack로 남는다(팬 마운트 이전/직후 창). 본 SPEC은 이 경로를 **수정하지 않는다** — R8 밸브가 흡수 대상으로 삼는 알려진 누수로 등재만 한다 |
| **알려진 잔여 누수 (b)** — ack IPC 실패 | `ackOutput` invoke가 거부되면 현행 코드가 예외를 삼키므로 그 배치의 ack이 영구 유실된다. 본 SPEC은 이 경로를 **수정하지 않는다** — 동일하게 R8 밸브 흡수 대상으로 등재한다 |

---

## §E 범위 제외 (Exclusions)

본 SPEC이 **의도적으로 만들지 않는** 것들이다. 미래의 독자가 "버그"로 오인하지 않도록 명시한다.

### Out of Scope — 워터마크 값 재조정

- `FLOW_HIGH_WATERMARK`(128KiB) / `FLOW_LOW_WATERMARK`(32KiB) / `RING_PAUSE_THRESHOLD`(768KiB) 값은 변경하지 않는다.
- 본 SPEC의 결함은 임계값이 아니라 **회계 단위**의 문제다. 단위를 고친 뒤에도 조정이 필요하다면 별도 후속 SPEC으로 계측 근거와 함께 다룬다.

### Out of Scope — 흐름 제어 구조 변경

- ack 배치 규율(`ACK_BATCH_BYTES` / `ACK_FLUSH_IDLE_MS`), reader park 메커니즘, `parsedSeq` 이원화, R16 상호 배제 구조는 변경하지 않는다.
- 본 SPEC은 기존 구조 위에서 회계 단위 정정과 밸브 1건 추가만 수행한다.

### Out of Scope — WebGL 컨텍스트 유실 재획득

- WebGL 유실 후 DOM 렌더러 강등 상태의 재획득 시도는 여전히 구현하지 않는다(SPEC-PTY-FLOW-001 §E와 동일).
- 강등 상태의 파싱 처리량 저하는 흐름 제어가 유한하게 흡수할 뿐이다.

### Out of Scope — IME 에코 패스스루 윈도우 강화

- `ECHO_PASS_MS` 윈도우 및 `lastInputTs` 갱신 규칙 개선은 별도 후속 SPEC이다.
- 본 SPEC의 IME 접점은 R4(보류 버퍼 `byteLen` 합산) 한 건으로 한정한다.

### Out of Scope — 비활성 워크스페이스 ring/replay 의미론 변경

- 비활성 워크스페이스의 ring 누적 + oldest-drop + `replay_pane` 재생 의미론은 변경하지 않는다.
- 워크스페이스 전환 시 복구 동작(R15 리셋 → emitter 재개)은 **보존 대상**이며 회귀 가드로 검증한다.

### Out of Scope — 프론트엔드 독립 바이트 산정 도입

- `TextEncoder` 등으로 프론트엔드가 자체 UTF-8 길이를 산정하는 대안은 검토 후 기각했다(R5).
- 두 개의 독립 산정 지점을 두면 인코딩 경계 사례에서 단위 불일치가 재발할 구조적 여지가 남는다. 단일 진실 원천(백엔드) + 반사 방식이 결함 클래스 자체를 제거한다.

### Out of Scope — 외부 API·프로토콜 표면 변경

- control-pipe API, automation rule engine, injection 게이트 API 표면은 변경하지 않는다.
- `PtyOutputEvent`의 `byteLen` 추가는 내부 Tauri 이벤트 전용이다.

---

## §F 참조

- `.moai/specs/SPEC-PTY-FLOW-002/plan.md` — 구현 계획, 마일스톤, 기술 결정
- `.moai/specs/SPEC-PTY-FLOW-002/acceptance.md` — 수용 기준, Given-When-Then
- `.moai/specs/SPEC-PTY-FLOW-002/research.md` — 근본 원인 분석과 file:line 증거
- `.moai/specs/SPEC-PTY-FLOW-001/` — 선행 SPEC(흐름 제어 도입). 본 SPEC은 그 후속 결함 수정이다
- `docs/ADR-014-pty-flow-control.md` — sync 단계에서 회계 단위 정정 + emitter 밸브를 반영하여 개정
- `docs/ARCHITECTURE.md` §6, `docs/DEVELOPMENT.md` — sync 단계에서 갱신
