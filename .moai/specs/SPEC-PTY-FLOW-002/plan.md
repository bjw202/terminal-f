# SPEC-PTY-FLOW-002 — 구현 계획 (plan)

- SPEC: `.moai/specs/SPEC-PTY-FLOW-002/spec.md` (GEARS R1~R13)
- Tier: M · cycle_type: **tdd** (재현-우선) · Route: A (Hybrid Trunk main-direct)
- depends_on: SPEC-PTY-FLOW-001 (`status: completed`)
- 추정 변경 파일 7건: `src-tauri/src/output.rs`, `src-tauri/src/flow_state.rs`, `src-tauri/src/flow_tests.rs`, `src-tauri/src/bin/bench.rs`, `src/terms.ts`, `src/types.ts`, `src/autotest.ts`
  - `bench.rs`는 plan-audit iteration 1의 F6(bench Phase A 10초 구간 ↔ 밸브 `stall_timeout` 경계 충돌) 완화를 위해 추가되었다. 변경 내용은 (i) 손수 조립하는 표본 JSON 2곳에 `emitter_valve_fired` 필드 추가(`bench.rs:318-327` Phase A, `bench.rs:358-366` Phase B), (ii) `bench.rs:263-271`의 config-주입 불가 주석 갱신이다.
  - **`stall_timeout` 확대의 실제 구현 지점은 `bench.rs`가 아니라 `flow_state.rs`**다 — `FlowConfig::default()`가 `TERMF_FLOW_STALL_TIMEOUT_MS`를 읽는 env 오버라이드이며, bench는 그 환경변수를 세팅해 실행할 뿐이다. 사유는 §B B10(registry 세션이 `FlowState::default()` 하드코딩이라 config 주입 seam이 없음).

---

## §A 기술 결정 (가역성 높은 순 — 바꿀 가능성이 큰 결정부터)

### A.1 이벤트 스키마 변경 — `PtyOutputEvent.byteLen` (가장 되돌리기 어려운 결정)

`src-tauri/src/output.rs:31-38`의 `PtyOutputEvent`에 `pub byte_len: usize` 필드를 추가한다. 구조체는 이미 `#[serde(rename_all = "camelCase")]`이므로 직렬화 키는 `byteLen`이 되고, `src/types.ts:229-235`의 대응 인터페이스에 `byteLen: number`를 추가한다.

값의 산출 시점: `output.rs:79-98`에서 오버플로 배너 접두어를 붙인 **최종** `data`가 확정된 직후, `let byte_len = data.len();`으로 한 번 계산하여 `record_emit(byte_len)`과 이벤트 필드 양쪽에 같은 값을 쓴다. 이렇게 하면 R2(동일 원천)가 코드 구조로 보장되며, 두 지점이 나중에 어긋날 여지가 없다.

대안 검토·기각:
- **프론트엔드 `TextEncoder` 자체 산정** — 산정 지점이 둘로 늘어 인코딩 경계에서 재발 여지가 남는다(spec R5, §E 기각 사유).
- **백엔드가 UTF-16 코드 유닛으로 회계 전환** — 백엔드가 Rust 문자열에서 UTF-16 길이를 세는 비용을 emit 경로마다 지불해야 하고(순회 필요), ring 용량·워터마크가 모두 바이트 축이라 축이 어긋난다.
- **ack 프로토콜을 seq 기반으로 전환** — 배압 신호로서의 "미확인 **바이트**"라는 의미가 사라진다. 구조 변경 폭이 크고 SPEC-PTY-FLOW-001 전체를 다시 여는 셈이다.

### A.2 프론트엔드 ack 수치의 출처 교체 (타입 인터페이스 변경)

세 지점이 바뀐다.

| 위치 | 현행 | 변경 후 |
|---|---|---|
| `src/terms.ts:200` (`writeOutput`) | `writeParsed(view, data, seq, seq !== undefined ? data.length : 0)` | 호출자로부터 전달받은 `byteLen`을 사용 (`seq === undefined` → 0 유지) |
| `src/terms.ts:212` (`appendOutput`, IME 보류) | `view.heldAckBytes += data.length` | `view.heldAckBytes += byteLen` |
| `src/terms.ts:232-237` (`flushOutput`) | `heldAckBytes`를 그대로 사용 | 변경 없음 — 합산 결과가 이미 올바른 단위가 됨 |

**시그니처 형태 (확정)**: `writeOutput(paneId, data, seq?: number, byteLen: number)` 형태의 *선택 파라미터 뒤 필수 파라미터*는 TypeScript에서 불가하므로, `seq`와 `byteLen`을 **하나의 객체로 묶어** 타입 수준에서 짝을 강제한다:

```ts
export function writeOutput(paneId: PaneId, data: string, meta?: { seq: number; byteLen: number }): void
```

- `meta`가 있으면 두 필드 모두 **필수**다 — `seq`만 넘기고 `byteLen`을 빠뜨리는 호출은 `tsc --noEmit`에서 컴파일 오류가 된다.
- `meta`가 없으면 합성 배너(exit / no session / overflow 메시지)이며 `ackBytes = 0`, `parsedSeq` 미전진이다(spec R6).
- 기존 호출부(`src/main.ts:337`, `src/main.ts:814`)는 `terms.writeOutput(paneId, ev.data, { seq: ev.seq, byteLen: ev.byteLen })`로, 합성 배너 호출부(`main.ts:324` / `main.ts:328` / `main.ts:348` / `main.ts:826`)는 `meta` 인자 없이 그대로 둔다.

**선택 파라미터 `byteLen?: number`를 기각한 이유**: 선택 파라미터는 미전달 시에도 타입 검사를 통과한다. 기존 호출부 `terms.writeOutput(paneId, ev.data, ev.seq)`를 고치지 않아도 `tsc --noEmit`이 exit 0을 반환하고, 런타임에서 `byteLen === undefined` → `writeParsed(..., undefined)` → `src/terms.ts:253`의 `if (ackBytes > 0)`이 false → **ack이 조용히 0건**이 되어 결손이 그대로 재발한다. 즉 이 SPEC이 고치려는 실패 모드가 선택 파라미터 아래에서는 컴파일러에 잡히지 않고 그대로 통과한다. 객체 묶음은 그 실패 모드를 컴파일 오류로 승격시킨다(§B B1의 완화책이 성립하는 유일한 형태다).

IME 보류 경로의 미묘한 지점: 보류 버퍼는 **여러 이벤트**를 모았다가 하나의 `term.write`로 flush한다(`terms.ts:222-237`). 따라서 `heldAckBytes`는 개별 이벤트 `byteLen`의 **합**이어야 하며, flush 시점에 이어붙인 문자열의 길이를 다시 재는 방식은 오답이다(R4/R5). 현행 코드가 이미 "누적 가산" 구조이므로 가산 대상 수치만 교체하면 되고 구조 변경은 없다.

`writeParsedNoAck`(`terms.ts:275-282`)는 `ackBytes = 0`으로 고정되어 있어 변경이 필요 없다 — replay·스냅샷·배너 미ack(R6)이 자동으로 유지된다.

### A.3 emitter 밸브 평가 지점 — `FlowState` 소유 상태 + `emitter_gate_decision` 내부 (채택)

**채택**: `FlowInner`에 두 필드를 추가하고 `flow_state.rs:171` `emitter_gate_decision` 안에서 판정한다.

```
paused_since: Option<Instant>   // 정지 진입 시각 (spec R7)
paused_at_acked: u64            // 정지 진입/마지막 진전 시점의 acked 스냅샷 (spec R7)
```

판정 규칙 — **무장(arming) 단계가 규칙 0이며 생략하면 밸브가 영구 미발화한다**:

0. **무장**: 이번 tick의 게이트 결정이 "정지"(반환 false)이고 `paused_since`가 `None`이면 → `paused_since = Some(now)`, `paused_at_acked = acked()`로 무장하고, **발화 판정은 다음 tick부터** 수행한다(이번 tick에는 발화하지 않는다). 이 규칙은 두 경로를 모두 덮는다 — (i) 방출 → 정지 **전이**(직전 `was_paused == false`), (ii) 전이 없이 이미 정지 상태로 진입한 경우(`was_paused == true`인데 `paused_since`가 아직 `None`). (ii)는 실제로 발생한다: 기존 테스트가 `emitter_gate_decision`을 상태 전이 없이 직접 호출하며(`src-tauri/src/flow_tests.rs:22-45`), 그 호출 패턴에서 `paused_since`가 `None`인 채 정지 상태가 관측된다. 규칙 0이 없으면 규칙 2가 평가 불가가 되어 M2 전체가 무효화된다.
1. **진전 리셋**: 정지 상태에서 현재 `acked()`가 `paused_at_acked`보다 크면 → 진전 있음: `paused_since = Some(now)`, `paused_at_acked = acked()`, 밸브 미발화 (spec R9).
2. **발화**: 진전이 없고 `paused_since`가 `Some(t)`이며 `now - t >= config.stall_timeout` → 밸브 발화: `reset_accounting()` + `emitter_valve_fired += 1` + 무장 해제(`paused_since = None`) + 방출 재개 (spec R8).
3. **해제**: 게이트 결정이 "방출"(반환 true)이면 `paused_since = None`, `paused_at_acked = 0`으로 초기화한다.

**이 지점을 고른 이유**: `emitter_gate_decision`은 `pump_once`가 16ms tick마다 세션별로 정확히 한 번 호출하는 **단일 결정 지점**이므로, 별도의 타이머 스레드나 tick 훅을 새로 만들 필요가 없다. 그리고 상태가 `FlowState`에 있으면 `flow_tests.rs`가 축소 `FlowConfig`(예: `stall_timeout = 60ms`)를 주입해 `SessionRegistry` 전체를 세우지 않고 결정론적으로 검증할 수 있다 — `flow_state.rs:37-58`에 이미 확립된 주입 패턴을 그대로 쓴다.

대안 검토·기각:
- **`output.rs` `pump_once` 안에서 판정** — 시각·acked 스냅샷을 어딘가에 보관해야 하는데 그 자리가 결국 `FlowState`다. 로직만 `output.rs`로 흩어져 테스트가 `SessionRegistry` 구성을 요구하게 된다(선행 교훈: 배선 의존 테스트는 비용이 크고 결함을 놓친다).
- **`record_ack`에서 역방향 판정** — ack이 아예 오지 않는 것이 결함의 정의이므로, ack 경로에 판정을 걸면 발화 조건에서 코드가 실행되지 않는다. 구조적으로 불가.
- **전용 감시 스레드** — 세션당 스레드 1개 추가. 기존 emitter tick이 이미 존재하는데 새 동시성 표면을 여는 것은 비용 대비 이득이 없다.

락 규율: `emitter_gate_decision`은 현재 `outstanding()`(원자 읽기)만 사용하지만, 밸브 판정을 위해 `inner` 뮤텍스를 짧게 잡아야 한다. `check_park_and_wait`와 달리 **블로킹 대기가 없는 짧은 임계 구역**이며, `notify_reader`가 이미 같은 뮤텍스를 lock-then-notify 방식으로 사용하고 있어 락 순서가 새로 생기지 않는다. registry 전역 락을 쥔 채 잡지 않는다(spec §C 준수 — `pump_once`는 `active_sessions_snapshot()`으로 스냅샷을 먼저 뜬 뒤 세션별로 진행한다).

### A.4 밸브 관측 노출 방식 (R10)

**현행 상태(작업 트리 확인)**: `FlowStats`는 Rust(`src-tauri/src/flow_state.rs:316-324`)·TS(`src/types.ts:125-131`) 모두 5필드(`emitted` / `acked` / `outstanding` / `emitterPaused` / `readerParked`)뿐이며, **밸브 발화 카운터는 노출되어 있지 않다**. `valve_fired: AtomicU64`는 내부 원자 카운터로만 존재하고(`flow_state.rs:95`) 외부 접근 경로는 `valve_fired_count()`(`flow_state.rs:284`) 하나이며 그 호출자는 테스트 2곳이 전부다. 즉 "기존 `valveFired` 필드"라는 것은 **존재하지 않는다** — 아래는 필드 보존이 아니라 신규 노출 결정이다.

**채택 (명시적 선택)**: `FlowStats`에 **두 필드를 신규 추가**한다 — Rust 구조체와 TS 인터페이스 양쪽에 동일하게.

| 신규 필드 (TS / Rust) | 원천 | 의미 |
|---|---|---|
| `valveFired` / `valve_fired` | 기존 내부 카운터를 **노출만** (값 계산 로직 무변경) | reader-park 밸브(SPEC-PTY-FLOW-001 R6) 발화 횟수 |
| `emitterValveFired` / `emitter_valve_fired` | 신규 카운터 | emitter 정지 밸브(본 SPEC R8) 발화 횟수 |

카운터를 **하나로 합치지 않는** 이유: §A.5의 autotest 판정 (f)가 "emitter 사각지대 밸브가 발화했는가"를 reader-park 밸브와 **구분해** 판정해야 하고(§A.3 사각지대가 본 SPEC의 대상), 합치면 그 구분이 사라져 판정 (f)가 성립하지 않는다. reader 밸브 카운터도 함께 노출하는 이유: 값이 이미 존재하므로 노출 비용이 사실상 0이고, 두 밸브를 나란히 관측해야 "어느 밸브가 구제했는가"를 판별할 수 있다.

무회귀 경계(R11): 기존 5필드의 **이름·타입·의미·값은 변경하지 않는다**. 추가는 append-only이며, `#[serde(rename_all = "camelCase")]`가 이미 붙어 있어 직렬화 키는 자동으로 camelCase가 된다.

### A.5 autotest 비ASCII 홍수 체크 설계 (R13)

`src/autotest.ts`의 FLOW-001 AC-9 홍수 체크(260~320행) 바로 뒤에 형제 체크를 추가한다. 기존 체크의 **로직과 판정은 손대지 않는다**(회귀 가드).

- 페이로드: PowerShell 루프로 한국어 + 박스 드로잉이 지배적인 행을 방출한다. 예: `$pad='─'*80; 1..4000 | ForEach-Object { "한글출력 $_ $pad" }; 'TERMF_U8FLOOD_DONE'` — 행당 약 250~330 UTF-8 바이트, 총 약 1.3MiB 규모. UTF-16 코드 유닛 대비 바이트 비율이 약 3배가 되어 결함이 있으면 결손이 빠르게 32KiB를 넘긴다.
- 표본: 기존 체크와 동일하게 `ipc.flowStats(pane)`를 폴링하여 `{emitted, acked, outstanding, emitterPaused, readerParked, valveFired, emitterValveFired}`를 수집한다.
- **기계 판정 6건**:
  - (a) `u8FloodAckProgress` — `acked` 전진
  - (b) `u8FloodOutstandingBounded` — `outstanding` 최대값 ≤ 512KiB
  - (c) `u8FloodTailRendered` — 꼬리 마커 `TERMF_U8FLOOD_DONE` 렌더
  - (d) `u8FloodNoPermanentPause` — 마지막 3표본이 연속 `emitterPaused === true`가 아님
  - (e) `u8FloodAckRatio` — 최종 표본의 `acked / emitted >= 0.9`
  - (f) `u8FloodNoValveRescue` — 홍수 구간 동안 `emitterValveFired` 증가량 0
- **(e)/(f)가 필수인 이유 (밸브 마스킹 차단)**: M2에서 밸브가 도입된 뒤에는 (a)~(d)만으로 결함을 잡지 못한다. UTF-16 회귀가 재발해도 밸브가 `stall_timeout`마다 회계를 리셋하면 `acked`는 전진하고(a), `outstanding`은 리셋으로 눌리며(b), 방출이 재개되어 꼬리도 렌더되고(c), 정지도 영구가 아니다(d) — 즉 §G가 경계한 "느린 팬 위장"이 종단 판정에서 실제로 성립한다. (e)는 반사 ack이 수치로 균형을 맞추고 있음을 증명하고(UTF-16 회귀 시 비율이 약 0.35로 떨어진다), (f)는 그 균형이 밸브 구제가 아님을 보장한다. 두 판정이 함께 있어야 종단 가드가 결함을 실제로 잡는다.
- **집계 편입 (F1 해소 — 명시적 결정)**: 신규 6체크는 **기존 `report.flowOk` 집계식을 확장**하여 편입한다. 신규 집계 필드를 만들지 않는다 — 판독 지점이 셋(`ok` / `flowOk` / 신규)으로 늘면 어느 것을 봐야 하는지가 다시 모호해진다.
  - 현행 `report.flowOk`(`src/autotest.ts:824-831`)는 `floodAckProgress && floodOutstandingBounded && floodNoOverflow && floodTailRendered && switchUnderLoadNoGap`의 5항 AND다. 기존 5항은 **문자 그대로 보존**하고 신규 6항만 AND로 덧붙인다(append-only).
  - `report.ok`의 32체크 체인은 **변경하지 않는다**. 흐름 제어 체크를 그 체인에 편입하면 `autotest.ts:817-822`가 명시한 분리 설계(흐름 제어 실패가 기존 32체크 판정을 가리지 않게 한다)를 깨뜨린다.
  - 따라서 본 SPEC의 autotest 판정 기준은 `report.ok`가 **아니라** `report.flowOk`이며, DoD는 두 필드를 **병기**로 확인한다(`ok === true` AND `flowOk === true`).
  - 이 집계식 수정은 §D PRESERVE / §G 금지 조항의 **명시적 예외**다: 보존 대상은 기존 체크의 **로직과 판정 결과**이지 집계식 자체가 아니다(§D·§G에 예외를 명문화).
- PowerShell 출력 인코딩: 콘솔 출력이 UTF-8로 나오는지 확인이 필요하다. 필요 시 명령 앞에 `[Console]::OutputEncoding=[Text.Encoding]::UTF8;`를 붙여 고정한다 — 인코딩이 깨지면 체크가 결함이 아니라 환경을 측정하게 된다.

### A.6 배너 회계 주석 정정 (R2 부수 작업)

`src-tauri/src/output.rs:90-92`의 주석 "배너 접두어 길이는 흐름 회계에서 제외(백엔드 발생분이 아니라 UI 힌트)"는 실제 코드(`record_emit(data.len())` — 배너 포함 문자열)와 어긋난다. §A.1에서 `byte_len`을 최종 문자열로부터 한 번만 산출하므로 emit 회계와 `byteLen`이 배너 포함으로 자기 정합해진다. 주석을 코드에 맞게 "배너 포함 최종 문자열 기준 — 이벤트 `byteLen`과 동일 원천(R2)"로 정정한다. 동작 변경은 없다(이미 배너 포함으로 동작 중).

### A.7 미해결 클래리피케이션 없음

`[NEEDS CLARIFICATION]` 항목 없음. §A.1~§A.6의 모든 설계 결정은 사용자 승인 완료(D1/D2/D3)이며 대안 기각 사유까지 기록되었다.

---

## §B 알려진 리스크 (Known Issues)

| # | 리스크 | 완화 |
|---|---|---|
| B1 | 이벤트 필드 추가로 백엔드/프론트 타입이 어긋나 런타임에서 `undefined` ack이 발생 | `types.ts`의 `PtyOutputEvent.byteLen`을 **필수 필드**로 선언하고, `writeOutput`의 `seq`/`byteLen`을 `meta?: { seq; byteLen }` **객체로 묶어** 짝을 타입 수준에서 강제한다(§A.2 확정). 선택 파라미터 형태는 이 완화책을 무력화하므로 기각했다 — 미전달이 컴파일 오류가 되어야만 `tsc --noEmit`이 실제 가드가 된다 |
| B2 | IME 보류 경로에서 `byteLen` 합산 누락 — 보류 이벤트가 여러 건일 때만 드러나는 조용한 결손 | AC-5 전용 검증(보류 2건 이상 → flush → ack 수치 = 두 `byteLen`의 합). grep으로 `heldAckBytes += data.length` 잔존 0건 확인 |
| B3 | emitter 밸브 오발화 — 느리지만 정상적인 팬의 미확인 구간을 회계상 삭제 | R9(진전 시 타이머 리셋) + AC-7 전용 부정 테스트(ack이 진전하는 동안 밸브 미발화) |
| B4 | 밸브가 결함을 가려 단위 통일 결함이 잠복 | 밸브는 M2에서 도입하고, M1의 재현 테스트는 **밸브 없이** 결함/수정을 판정하도록 축소 config에서 `stall_timeout`을 충분히 크게 잡는다. AC-2/AC-3는 밸브와 독립 |
| B5 | `emitter_gate_decision`에 뮤텍스 도입으로 16ms tick 경로에 락 경합 | 임계 구역은 원자 읽기 + 필드 2개 갱신뿐이며 블로킹 대기가 없다. bench 홍수 경로로 성능 회귀 부재 확인(AC-13) |
| B6 | PowerShell autotest 페이로드의 콘솔 인코딩이 UTF-8이 아니어서 체크가 환경을 측정 | §A.5의 `[Console]::OutputEncoding` 고정. 체크 실패 시 결함 판정 전에 인코딩을 먼저 확인하는 절차를 체크 메시지에 남긴다 |
| B7 | autotest를 terminal-f 팬 안에서 실행하여 세션이 자체 종료 | 팬 밖에서 실행. 리포트 파일(`src-tauri/autotest-report.json`)이 정본 |
| B8 | Windows Git Bash 최소 PATH에서 `detect_shell_finds_something_on_windows` 환경 실패 | 선행 SPEC에서 확인된 기존 환경 이슈(회귀 아님). PowerShell에서 `cargo test` 권장 |
| B9 | 변경 파일이 SPEC-PTY-FLOW-001과 동일 집합이라 스코프가 번지기 쉬움 | §D PRESERVE 목록 엄수. 워터마크 값·ack 배치 규율·reader park 구조는 손대지 않는다 |
| B10 | **bench Phase A와 밸브 `stall_timeout` 경계 충돌** — bench의 Phase A는 정확히 10초 동안 ack을 전혀 합성하지 않으면서 `pump_once`를 50ms마다 구동하고(`src-tauri/src/bin/bench.rs:307-329`), `stall_timeout`은 기본 10초다. M2 밸브 도입 시 발화 시점이 Phase A 종료 경계 부근이 되어 `outstanding_at_end_a`와 Phase B 진입 기준선(`ring_drop_before_b`)이 흔들린다 | **완화책 확정 (env 오버라이드)**: `FlowConfig::default()`가 환경변수 `TERMF_FLOW_STALL_TIMEOUT_MS`를 1회 읽어 `stall_timeout`을 오버라이드하게 하고(파싱 실패·미설정 시 기본 10초 **불변**), bench를 `TERMF_FLOW_STALL_TIMEOUT_MS=60000`으로 실행한다. 구현 지점은 `src-tauri/src/flow_state.rs`의 `FlowConfig::default()` 한 곳이다.<br>**`with_config` 주입을 기각한 이유**: registry가 만드는 세션은 `FlowState::default()`로 **하드코딩**되어 있고(`src-tauri/src/session.rs:522`, `src-tauri/src/session.rs:1217`) `Arc<PtySession>` 배후라 생성 후 config 필드를 바꿀 수도 없다. `FlowState::with_config`는 standalone 인스턴스 전용이며, bench 자신이 이 불가능성을 코드 주석으로 이미 기록해 두었다(`src-tauri/src/bin/bench.rs:263-271`). 즉 "bench가 config를 주입한다"는 계획은 `session.rs`에 신규 주입 seam을 내지 않는 한 실행 불가다 — 그 seam은 §D PRESERVE 위반이자 스코프 확장이다.<br>env 오버라이드는 **신규 임계값 상수가 아니다**(§C 상수 규율 위반 아님) — 기본값은 그대로 `FLOW_STALL_TIMEOUT`이고 테스트·계측용 오버라이드 창구만 연다. 밸브 자체의 검증은 축소 config 단위 테스트(AC-6/AC-7)가 담당한다 |
| B11 | **알려진 잔여 누수 (a)** — `writeOutput`이 미마운트 팬(`views.get` 실패)에서 조기 반환하여, 이미 emit 회계에 가산된 바이트가 영구 미ack로 남는다(마운트 이전/직후 창) | 본 SPEC 범위에서 **수정하지 않는다**. R8 밸브가 흡수하는 알려진 누수로 spec.md §D·research.md §B에 등재하여 "미지"로 남기지 않는다. 실사용에서 밸브 발화가 관측되면 이 경로가 1순위 조사 대상이다 |
| B12 | **알려진 잔여 누수 (b)** — `ackOutput` invoke 실패를 현행 코드가 삼켜(`src/terms.ts:313` `.catch(...)`) 해당 배치의 ack이 영구 유실된다 | B11과 동일 — 수정하지 않고 등재만 한다. 실패가 조용하다는 점이 위험 요소이므로, 후속 SPEC 후보로 `flow_stats`의 `emitterValveFired` 추이를 관찰 지표로 삼는다 |

---

## §C 사전 점검 (Pre-flight)

```bash
# 1. Rust 테스트 baseline (135건 green 확인 — NEW 실패 구분용)
cd src-tauri && cargo test 2>&1 | tail -20

# 2. clippy baseline (NEW 경고 vs 기존 구분용)
cd src-tauri && cargo clippy --all-targets 2>&1 | tail -20

# 3. 프론트엔드 타입 baseline
npx tsc --noEmit; echo "exit=$?"

# 4. 결함 지점 현행 확인 (변경 대상 3곳)
grep -n "record_emit" src-tauri/src/output.rs
grep -n "data.length" src/terms.ts
grep -n "PtyOutputEvent" -A 8 src/types.ts

# 5. 기존 밸브/리셋 접점 확인 (무회귀 대상)
grep -n "fire_valve_internal\|reset_accounting\|disarm_for_teardown" src-tauri/src/flow_state.rs src-tauri/src/session.rs

# 6. autotest baseline (기존 체크 전부 green — 팬 밖에서 실행, 리포트가 정본)
#    TERMF_AUTOTEST=1 실행 후 src-tauri/autotest-report.json 판독
```

---

## §D 제약 (DO NOT VIOLATE)

PRESERVE 목록 — 다음은 **변경 금지**다.

- `FLOW_HIGH_WATERMARK` / `FLOW_LOW_WATERMARK` / `RING_PAUSE_THRESHOLD` / `FLOW_STALL_TIMEOUT` / `ACK_BATCH_BYTES` / `ACK_FLUSH_IDLE_MS` / `SNAPSHOT_DRAIN_TIMEOUT_MS`의 **값**
- `check_park_and_wait` / `should_reader_park` / `fire_valve_internal`의 기존 동작(R6 reader 밸브)
- `disarm_for_teardown`(R8) 및 teardown join 계약
- `reset_accounting` 호출 3지점 (`session.rs:684-686`, `session.rs:803-807`, 밸브 발화 시)
- `pump_once`의 R16 락 범위(collect + `last_emitted_seq.store`가 동일 ring 락 안)
- `writeParsedNoAck` 경로와 그 호출부(replay·스냅샷·배너 미ack)
- `receivedSeq` / `parsedSeq` 이원화 및 `replayInFlight` 지연 버퍼
- 기존 autotest 체크의 **로직과 판정 결과** (특히 FLOW-001 AC-9 ASCII 홍수, FLOW-001 AC-10a 전환-중-부하)
- `report.ok`의 32체크 AND 체인 (`src/autotest.ts:786-815`) — 흐름 제어 체크를 이 체인에 편입하지 않는다
- `FlowStats` 기존 5필드의 이름·타입·의미·값 (`emitted` / `acked` / `outstanding` / `emitterPaused` / `readerParked`)
- control-pipe API, automation engine, injection 게이트 API 표면

> **PRESERVE 예외 (명문화)**: `report.flowOk` 집계식(`src/autotest.ts:824-831`)의 **확장은 허용된다** — 기존 5항을 문자 그대로 보존한 채 신규 항만 AND로 덧붙이는 append-only 수정이다(§A.5). 보존 대상은 기존 체크의 로직과 판정 결과이지 집계식 자체가 아니다. 마찬가지로 `FlowStats`에 대한 **필드 추가**는 허용된다(기존 5필드 불변, append-only).

금지 명령: `git push --no-verify`, `git commit --amend`(푸시 후), `git push --force`, `git add -A`(명시 경로만 스테이징).
필수: Conventional Commits (`fix(SPEC-PTY-FLOW-002): M{N} <subject>`), `🗿 MoAI` 트레일러, 커밋 메시지 한국어(`git_commit_messages: ko`).

---

## §E 자기 검증 (Self-Verification)

각 마일스톤 완료 보고는 검증 주장 무결성 5절 형식(주장 / 증거 / baseline 귀속 / 미검증 / 잔여 위험)을 따른다. 최소 제출물:

| 항목 | 명령 | 기대 |
|---|---|---|
| E1 AC 매트릭스 | acceptance.md AC별 PASS/FAIL + 검증 명령 + verbatim 출력 | 전 MUST PASS |
| E2 RED 증거 (TDD) | 수정 **이전** 재현 테스트 실행의 실패 출력 verbatim | 결함 재현 확인 |
| E3 Rust 테스트 | `cargo test` | 기존 135 + 신규 전부 green, NEW 실패 0 |
| E4 clippy | `cargo clippy --all-targets` | NEW 경고 0 |
| E5 타입 검사 | `npx tsc --noEmit` | exit 0 |
| E6 잔존 결함 grep | `grep -n "data.length" src/terms.ts` | 매치가 **정확히 1건**(`outBufLen` 계산, `terms.ts:210`)만 남고 ack 경로 유래 매치 0건 |
| E7 autotest | 팬 밖 실행 → `src-tauri/autotest-report.json` | **`ok === true` AND `flowOk === true`** 병기 확인. `flowOk`는 기존 5항 + 신규 6항 모두 true여야 성립 (`ok`만 확인하는 것은 흐름 제어를 전혀 측정하지 않는 것과 같다 — `ok` 체인은 흐름 제어 체크를 구조적으로 배제한다) |
| E8 커밋/푸시 | `git log --oneline`, `git push` 결과 | 마일스톤별 커밋 SHA 명시 |

---

## §F 마일스톤 (독립 검증 가능 단위)

2개 마일스톤으로 나눈다. **근거**: M1이 1차 정합성 수정(단위 통일)과 그 재현 증거를 독립적으로 확정한 뒤, M2가 방어 심화(밸브)와 종단 검증(autotest)을 얹는다. 밸브를 M1과 섞으면 밸브가 결함을 가려 "수정이 실제로 동작했는가"를 판정할 수 없게 된다(B4).

### M1 — 단위 통일 + 재현-우선 회귀 테스트 (spec R1~R6, R12)

- **RED**: `src-tauri/src/flow_tests.rs`에 배선 수준 재현 테스트를 추가한다. 비ASCII 페이로드를 `pump_once` 경로로 방출하고, 이벤트 `data`의 **UTF-16 코드 유닛 수**로 ack하여(구 프론트엔드 동작 모사) `outstanding`의 바닥값이 누적 결손만큼 남고 `emitter_gate_decision`이 정지 상태로 고착됨을 단언한다. 짝 테스트는 동일 경로를 이벤트 `byteLen`으로 ack하여 `outstanding <= low_watermark` 도달 + 방출 재개를 단언한다(수정 전에는 두 번째가 실패 — `byteLen` 필드 자체가 없으므로 컴파일 실패가 RED의 첫 형태다).
- **GREEN**: `output.rs` — `PtyOutputEvent.byte_len` 추가 + 최종 문자열에서 1회 산출하여 `record_emit`과 이벤트에 동일 값 사용(§A.1) + §A.6 주석 정정. `types.ts` — `PtyOutputEvent`에 `byteLen: number`(필수) 추가. `terms.ts` — `writeOutput`을 `meta?: { seq; byteLen }` 형태로 전환(§A.2), 200행·212행의 `data.length` 제거. `main.ts` — 실 PTY 이벤트 호출부 2곳은 `meta` 전달, 합성 배너 호출부는 `meta` 미전달 유지.
- **주석 정리 (AC-4 기준 성립 조건)**: `src/terms.ts:199`의 주석 — 현재 `// ackBytes: seq 가 있는 배치(실 PTY 출력)만 data.length 만큼 ack 누적.` — 을 함께 고쳐 `data.length` 언급을 제거한다(예: `// ackBytes: seq 가 있는 배치(실 PTY 출력)만 이벤트 byteLen 만큼 ack 누적.`). 이 주석을 남기면 `grep -n "data.length" src/terms.ts` 매치가 2건이 되어 **올바른 코드에서 AC-4가 실패한다**. 코드 수정과 함께 처리해야 하는 필수 산출물이며 선택적 정리가 아니다.
- 검증: AC-1, AC-2, AC-3, AC-4, AC-5, AC-9, AC-12, AC-16.

### M2 — emitter 정지 안전밸브 + 비ASCII 홍수 autotest (spec R7~R11, R13)

- **RED**: 축소 `FlowConfig`(작은 워터마크 + 짧은 `stall_timeout`) 주입 테스트 — (a) emitter 정지 + ack 무진전 → `stall_timeout` 경과 후 밸브 발화·회계 리셋·방출 재개, (b) ack이 진전하는 동안 밸브 미발화, (c) `flow_stats`에 발화가 노출.
- **GREEN**:
  - `flow_state.rs` — `FlowInner`에 `paused_since` / `paused_at_acked` 추가, `emitter_gate_decision`에 §A.3 판정 규칙 0~3 삽입, `emitter_valve_fired` 카운터 추가, Rust `FlowStats`에 `valve_fired` + `emitter_valve_fired` 두 필드 추가(§A.4). `@MX:WARN`(밸브) / `@MX:ANCHOR`(반사 ack 계약) 부착.
  - **`src/types.ts` — `FlowStats` 인터페이스에 `valveFired: number` + `emitterValveFired: number` 추가**. 이 항목이 빠지면 §A.5의 표본 수집 코드가 `tsc --noEmit`에서 실패한다(AC-12 red). `types.ts`는 이미 §변경 파일 목록에 있으므로 스코프 확장이 아니다.
  - `flow_state.rs` (B10 완화책) — `FlowConfig::default()`가 `TERMF_FLOW_STALL_TIMEOUT_MS`를 **1회** 파싱하여 `stall_timeout`을 오버라이드. 미설정·파싱 실패 시 기본 `FLOW_STALL_TIMEOUT`(10s) 불변.
  - `bench.rs` — 손수 조립하는 표본 JSON **2곳**(`bench.rs:318-327` Phase A, `bench.rs:358-366` Phase B)에 `"emitter_valve_fired": s.emitter_valve_fired` 추가. 이 두 블록은 `FlowStats` 구조체를 직렬화하는 것이 아니라 필드를 손으로 나열하므로, Rust `FlowStats`에 필드를 추가해도 **자동으로 따라오지 않는다** — 누락 시 AC-13의 `emitter_valve_fired == 0` 판정이 대상 필드 부재로 성립하지 않는다. `bench.rs:263-271`의 config-주입 불가 주석도 env 오버라이드 도입에 맞게 갱신한다.
- **autotest**: `autotest.ts`에 §A.5 비ASCII 홍수 체크 6판정 추가 + `report.flowOk` 집계식 append-only 확장. 기존 체크 로직·판정 무변경.
- 검증: AC-6, AC-7, AC-8, AC-10a~f, AC-11, AC-13, AC-14.

### §F.S — sync 단계 산출물

- `docs/ADR-014-pty-flow-control.md` 개정 — 회계 단위(UTF-8 바이트, 백엔드 단일 원천 + 반사 ack) 명시, emitter 밸브를 R6 reader 밸브와 나란히 기술, 사각지대와 그 해소 서술.
- `docs/ARCHITECTURE.md` §6 갱신, `docs/DEVELOPMENT.md` 모듈 지도 갱신.
- `CHANGELOG.md` `[Unreleased]`에 SPEC-PTY-FLOW-002 엔트리.
- spec.md frontmatter `in-progress → implemented → completed` 전이 + progress.md §E.4.
- 검증: AC-15.

---

## §G 안티패턴 (금지)

- **`record_ack`를 이미 올바른 바이트 수로 직접 호출하는 단위 테스트만으로 R12를 충족했다고 보고** — 배선을 우회해 결함 클래스를 은폐하는 정확히 그 패턴이다(§A.5, 선행 R4 교훈).
- **RED 없이 수정부터 작성** — 재현 실패 출력이 없으면 "이 결함이 실제로 이 코드 경로에서 발생한다"는 증거가 사라진다.
- **밸브를 1차 해법으로 취급** — 밸브가 있으면 단위가 어긋나도 10초마다 복구되어 결함이 "느린 팬"으로 위장된다. 밸브는 최종 방어선이다(spec R8).
- **프론트엔드에서 바이트 길이 재산정**(R5 위반).
- **워터마크 값 조정으로 증상 회피** — 근본 원인은 임계값이 아니라 단위다.
- **기존 autotest 체크의 로직·판정 수정** — 회귀 가드를 고치면 회귀 가드가 아니다. 신규 체크는 형제로 추가한다. (단 `report.flowOk` 집계식의 append-only 확장은 §D PRESERVE 예외로 명시적으로 허용된다 — §A.5.)
- **`report.ok`만 보고 autotest를 판정** — `ok` 체인은 흐름 제어 체크를 구조적으로 배제하므로, 신규·기존 흐름 체크가 전부 false여도 `ok:true`가 나온다. 판정은 반드시 `flowOk`를 병기한다.
- **흐름 제어 체크를 `report.ok` 체인에 편입** — 흐름 제어 실패가 기존 32체크 판정을 가리지 않게 한 분리 설계를 깨뜨린다.
- **`acked/emitted` 비율·밸브 발화 판정 없이 홍수 체크를 통과 처리** — 밸브 도입 후에는 (a)~(d)만으로 결함이 통과한다(§A.5).
- **실시간 10초 대기에 의존하는 밸브 테스트** — 축소 `FlowConfig` 주입을 사용한다.
- **`paused_since` 무장 단계 생략** — 규칙 0이 없으면 밸브가 영구 미발화하여 M2가 통째로 무효가 된다(§A.3).

---

## §H 참조

- `.moai/specs/SPEC-PTY-FLOW-002/spec.md` — 요구사항 R1~R13
- `.moai/specs/SPEC-PTY-FLOW-002/acceptance.md` — AC-1~AC-16 (논리 16건, AC-10은 sub-ID a~f를 1건으로 계수)
- `.moai/specs/SPEC-PTY-FLOW-002/research.md` — 근본 원인 분석 + file:line 증거표
- `.moai/specs/SPEC-PTY-FLOW-001/` — 선행 SPEC (R1~R16, AC-1~AC-15)
- `docs/ADR-004-backpressure-ring-buffer.md` — 유지되는 불변식
- `docs/ADR-014-pty-flow-control.md` — sync 단계에서 개정
