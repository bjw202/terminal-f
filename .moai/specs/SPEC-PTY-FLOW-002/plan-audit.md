# SPEC 감사 보고서: SPEC-PTY-FLOW-002

Iteration: 1/3
Verdict: **FAIL**
Overall Score: **0.71** (조화평균) / 0.75 (산술평균) — Tier M PASS 임계값 **0.80** 미달
감사 일자: 2026-08-18 · 감사자: plan-auditor (독립 적대적 감사)

> Reasoning context ignored per M1 Context Isolation. 사용자 승인 설계 결정(D1/D2/D3)은 **설계 방향 자체를 재개방하지 않는다**는 전제로만 수용했으며, 그 결정들이 아티팩트에 **정확히·검증 가능하게** 반영되었는지는 독립적으로 판정했다. 모든 file:line 주장은 작업 트리에서 직접 읽어 확인했다.

---

## Must-Pass 결과

| # | 기준 | 판정 | 증거 |
|---|---|---|---|
| MP-1 | REQ 번호 일관성 | **PASS** | `spec.md:80~148` — R1~R13 순차, 결번·중복 0건, zero-padding 규약 일관(무패딩, 선행 SPEC-PTY-FLOW-001 R1~R16과 동일 규약) |
| MP-2 | GEARS 형식 준수 (요구사항 레이어) | **PASS** | 판정 대상은 `spec.md §B`의 `R1~R13` **요구사항 레이어**이며 `acceptance.md`의 AC는 검증 레이어이므로 본 기준에서 채점하지 않음. R1/R2/R10/R11/R12/R13 Ubiquitous(`The X shall …`), R3/R8/R9 Event-driven(`When …, the X shall …`), R4/R7 State-driven(`While …, the X shall …`), R5/R6 Unwanted(`shall not`) — 5개 패턴 전부 정합. R8의 라벨 표기만 비정본(F13, INFO) |
| MP-3 | YAML frontmatter 유효성 | **PASS** | `spec.md:2~13` — canonical 12필드 전량 존재·타입 정합(`version: "0.1.0"` 인용 semver, `status: draft` enum, `created`/`updated` ISO, `priority: P0`, `phase: "v0.1.3 target"`(라이프사이클 토큰 아님), `lifecycle: spec-anchored`, `tags` 콤마 문자열). 거부 별칭(`created_at`/`updated_at`/`labels`/`spec_id`) 0건. 선택 필드 `tier: M`, `depends_on: [SPEC-PTY-FLOW-001]` 타입 정합 |
| MP-4 | 언어 중립성 (§22) | **N/A** | 단일 프로젝트(Rust + TypeScript) 고유 코드 SPEC이며 템플릿 바인딩/범용 다국어 툴링 표면 아님 → 자동 PASS |
| MP-5 | D7 교차 SPEC 정합 | **PASS** | 참조 SPEC 추출 결과 `SPEC-PTY-FLOW-001`, `SPEC-PTY-FLOW-002` 2건. `.moai/specs/SPEC-PTY-FLOW-001/spec.md:5` → `status: completed` — retired/superseded/archived 아님 → BLOCKING 없음 |
| MP-6 | D8 크로스 플랫폼 규율 | **PASS** | `grep -c 'syscall' spec.md plan.md acceptance.md` → 전부 `0` → 자동 PASS |
| MP-7 | 클래리피케이션 게이트 | **PASS** | `grep -rn '\[NEEDS CLARIFICATION' plan.md research.md` → 미해결 마커 0건. `plan.md:81` / `progress.md:11`의 매치는 "잔여 0건"을 **선언하는 메타 문장**이며 미해결 항목이 아님 |

**Must-Pass 전항 통과.** FAIL 판정의 근거는 must-pass 실패가 아니라 **집계 점수의 Tier M 임계값(0.80) 미달**이며, 그 원인은 아래 BLOCKER 1건 + MAJOR 4건이다.

---

## 카테고리 점수 (루브릭 기준)

| 차원 | 점수 | 밴드 | 증거 |
|---|---|---|---|
| Clarity | 0.75 | 0.75 (한두 요구사항에 경미한 모호성) | 요구사항 자체는 명확(`spec.md:80~148`). 감점 사유는 계획 레이어의 두 지점 — `plan.md:33`의 선택 파라미터 `byteLen?: number`가 `plan.md:89`(B1)의 "컴파일 시점 포착" 주장과 상충(F4), `plan.md:48~51`의 밸브 판정 규칙이 **정지 진입 시 `paused_since` 무장 단계**를 명시하지 않음(F7) |
| Completeness | 1.0 | 1.0 | HISTORY(`spec.md:20~24`), WHY(§A `spec.md:28~64`), WHAT(§B `spec.md:68~150`), 제약(§C), 엣지(§D), Out of Scope(§E `spec.md:196~229` — `### Out of Scope — <topic>` H3 7건 + 각 항목 `-` 불릿 존재, OutOfScopeRule 충족), 참조(§F). HOW는 `plan.md` §A~§H. Tier M 3-아티팩트 + research.md/progress.md 추가 제공 |
| Testability | 0.50 | 0.50 (복수 AC가 판단 개입 또는 잘못된 대상을 측정) | AC-11/§D.2가 측정 대상을 잘못 지목(F1, BLOCKER — `report.ok`는 흐름 제어 체크를 **구조적으로 배제**), AC-8이 존재하지 않는 필드를 전제(F2), AC-13이 "성능 회귀 없음"을 주장하나 기능 플래그만 확인(F9), AC-4는 grep 출력에 사람 필터가 필요(F11), R12의 실제 결함 지점(TS 배선)에 기계적 가드 부재(F5) |
| Traceability | 0.75 | 0.75 (1개 REQ 미커버) | R1→AC-1/12, R2→AC-1, R3→AC-3/4/12, R4→AC-5, R5→AC-4, R7·R8→AC-6, R9→AC-7, R10→AC-8, R11→AC-9/11/13, R12→AC-2/3, R13→AC-10a~d로 추적 가능. **R6(replay·스냅샷·합성 배너 미ack)만 §D AC 매트릭스에 대응 AC 없음** — §C 엣지 표(`acceptance.md:52~53`)의 grep 항목은 AC-ID를 갖지 않아 DoD 체크리스트에 편입되지 않음(F3) |

집계: 조화평균 4 / (1/0.75 + 1/1.0 + 1/0.50 + 1/0.75) = **0.706**, 산술평균 **0.75**. 두 방식 모두 Tier M 임계값 0.80 미달.

---

## 발견 사항 (Defects Found)

### F1 — `acceptance.md:79` (AC-11), `acceptance.md:104` (§D.2), `plan.md:158` (E7) — Severity: **BLOCKER** — Class: blocking

**주장**: "autotest 리포트 `ok:true`"가 비ASCII 홍수 체크 및 기존 흐름 제어 회귀 가드의 판정 기준이다.

**반증(관측)**: `src/autotest.ts:786~815`의 `report.ok` 체인은 32개 체크를 명시 열거하며 **흐름 제어 체크를 하나도 포함하지 않는다**. 해당 코드에 그 배제가 의도적임이 주석으로 명시되어 있다(`src/autotest.ts:817~822`: "기존 `report.ok` (32 체크) 체인에는 포함시키지 않는다"). 흐름 제어 집계는 별도 필드 `report.flowOk`(`src/autotest.ts:824~831`: `floodAckProgress && floodOutstandingBounded && floodNoOverflow && floodTailRendered && switchUnderLoadNoGap`)이며, 앱 종료 코드도 `report.ok`만 사용한다(`src/autotest.ts:838`).

**귀결**: AC-11과 §D.2 DoD를 문자 그대로 실행하면, **비ASCII 홍수 체크 4건(AC-10a~d)과 기존 홍수 체크 4건이 전부 false여도 `ok:true`가 나온다.** 본 SPEC의 유일한 종단(end-to-end) 회귀 가드가 아무것도 측정하지 못한다. 또한 `flowOk`라는 식별자는 5개 아티팩트 어디에도 등장하지 않는다(`grep -rn "flowOk" .moai/specs/SPEC-PTY-FLOW-002/` → 매치 0건; `acceptance.md:81`의 `flow_ok`는 bench 리포트 키 `src-tauri/src/bin/bench.rs:402`로 별개 대상).

**부수 모순**: 신규 체크를 어떤 집계에도 반영하려면 `report.flowOk` 대입식을 수정해야 하는데, `plan.md:138`(PRESERVE "기존 autotest 체크 전체")과 `plan.md:197`(§G "기존 autotest 체크 수정" 금지)이 그 편집을 금지하는 것으로 읽힌다. 신규 집계 필드 신설인지 기존 집계 확장인지가 미결정 상태다.

**필수 수정**: (1) AC-10a~d와 AC-11의 판정 기준을 `report.ok` → `report.flowOk`(또는 신설 `u8FlowOk`)로 정정한다. (2) 신규 4체크를 어느 집계 필드에 편입할지 `plan.md §A.5`에 명시하고, 그 집계식 수정이 §D PRESERVE/§G 금지 조항의 예외임을 명문화한다(체크 로직 무변경 ≠ 집계식 무변경). (3) §D.2 DoD의 "`ok:true`" 항목을 두 필드(`ok` + `flowOk`) 병기로 정정한다.

---

### F2 — `acceptance.md:73` (AC-8), `plan.md:64` (§A.4) — Severity: MAJOR — Class: blocking

**주장**: "기존 `valveFired`(reader 밸브) 필드의 의미·값은 변하지 않는다"(AC-8), "기존 `valveFired` 필드는 의미·값 모두 그대로 두어 무회귀를 지킨다(R11)"(§A.4).

**반증(관측)**: `FlowStats`에는 `valveFired` 필드가 **존재하지 않는다**. Rust 측 `src-tauri/src/flow_state.rs:318~324`의 `FlowStats`는 `emitted / acked / outstanding / emitter_paused / reader_parked` 5필드뿐이고, TS 측 `src/types.ts:125~131`도 동일하게 5필드다. `valve_fired` 카운터는 `flow_state.rs:95`에 존재하나 노출 경로는 `valve_fired_count()`(`flow_state.rs:284`)뿐이며, 이 메서드의 호출자는 테스트 2곳(`flow_tests.rs:178`, `flow_tests.rs:205`)이 전부다(`grep -rn "valve_fired\|valveFired"` 전수 확인).

**귀결**: (a) AC-8 후반부("기존 `valveFired` 필드의 의미·값 불변")는 존재하지 않는 대상에 대한 판정이므로 **검증 불가**하다. (b) AC-8의 검증 방법 "autotest 표본의 필드 존재 확인"은, reader 밸브 필드를 **새로** 노출하지 않는 한 성립하지 않는다. (c) §A.4의 "공유 vs 별도 필드" 설계 논증은 전제 하나(기존 노출 필드가 있다)가 틀린 채로 결론(별도 필드 채택)에 도달했다 — 결론 자체는 여전히 타당하나 근거 서술은 정정이 필요하다.

**필수 수정**: AC-8을 "`flow_stats` 응답에 emitter 밸브 발화 카운터가 신규 노출되고, 기존 5필드(`emitted/acked/outstanding/emitterPaused/readerParked`)의 의미·값이 불변"으로 재작성하거나, reader 밸브 카운터도 함께 노출한다면 그 신규 노출을 R10/§A.4에 명시한다. `plan.md §A.4`의 "기존 `valveFired` 필드" 서술을 "기존 `valve_fired` **내부 카운터**(미노출)"로 정정한다.

---

### F3 — `acceptance.md:64~83` (§D AC 매트릭스) — Severity: MAJOR — Class: blocking

**관측**: §D AC 매트릭스의 "요구사항" 열 전수 확인 결과 **R6에 대응하는 AC가 없다**. R6(`spec.md:110~114` — replay·스냅샷·합성 배너 미ack 유지, 선행 SPEC R13 불변식)은 §C 엣지 표(`acceptance.md:52~53`)에 grep 항목으로만 존재하며, §C 항목은 AC-ID가 없어 §D.2 DoD 체크리스트("AC-1 ~ AC-12 전부 PASS")에 편입되지 않는다.

**위험**: R6은 회귀 가드 성격의 요구사항이다. `writeOutput` 시그니처에 `byteLen`을 추가하는 변경(§A.2)은 합성 배너 호출부(`src/main.ts:348`, `src/main.ts:826` — `seq` 없음)와 replay 경로(`src/main.ts:300/309/317` — `writeParsedNoAck`)를 인접에서 건드리므로, R6 위반은 정확히 이번 변경에서 발생 가능한 회귀다. 그런데 DoD가 이를 강제 확인하지 않는다.

**필수 수정**: R6 전용 AC를 신설한다(예: AC-16 — `grep -n "writeParsedNoAck" src/main.ts` 호출부 전량이 `ackBytes = 0` 경로 경유 + `writeOutput` 합성 배너 호출부에 `byteLen` 인자 미전달, 매치 수 명시). 또는 AC-4를 확장하여 R6을 요구사항 열에 추가하고 기준문에 배너·replay 경로를 포함시킨다.

---

### F4 — `plan.md:33` (§A.2) vs `plan.md:89` (§B B1) — Severity: MAJOR — Class: blocking

**관측**: §A.2는 `writeOutput(paneId, data, seq)`의 시그니처에 **선택 파라미터** `byteLen?: number`를 추가한다고 명시한다. §B B1은 같은 리스크(전달 누락 → `undefined` ack)의 완화책으로 "`tsc --noEmit`으로 전달 누락을 **컴파일 시점**에 잡는다"고 주장한다.

**반증**: 선택 파라미터는 미전달 시에도 타입 검사를 통과한다. `terms.writeOutput(paneId, ev.data, ev.seq)`(현행 `src/main.ts:337`, `src/main.ts:814`) 형태를 그대로 두어도 `tsc --noEmit`은 exit 0이며, 런타임에서 `byteLen === undefined` → `writeParsed(..., undefined)` → `if (ackBytes > 0)`(`src/terms.ts:253`) false → **ack가 조용히 0건**이 되어 결손이 다시 누적된다. 즉 B1이 막겠다고 선언한 실패 모드가 채택 시그니처에서는 그대로 통과한다. B1의 완화책은 채택 설계와 상충한다.

**필수 수정**: 다음 중 하나를 §A.2에 확정한다. (a) `byteLen`을 **필수 파라미터**로 선언하고 합성 배너 호출부는 명시적으로 `0`을 전달한다. (b) `writeOutput(paneId, data, meta?: { seq: number; byteLen: number })` 형태로 `seq`와 `byteLen`을 **하나의 객체로 묶어** 타입 수준에서 짝을 강제한다(B1의 "seq와 짝으로 다룬다"는 서술과 정합). (c) 선택 파라미터를 유지한다면 B1의 완화책을 "컴파일 시점 포착"이 아닌 실제 가드(런타임 assert 또는 F3에서 신설할 AC의 grep)로 교체한다.

---

### F5 — `spec.md:140~144` (R12), `plan.md:169` (M1 RED), `acceptance.md:67~68` (AC-2/AC-3) — Severity: MAJOR — Class: blocking

**관측**: R12는 "배선(이벤트 페이로드 → ack 수치 산출)을 우회하는 테스트는 요구사항을 충족하지 않는다"고 못 박는다. 그런데 M1 RED가 지정하는 재현 테스트 위치는 `src-tauri/src/flow_tests.rs`(Rust)다. 결함이 실제로 사는 배선은 **TypeScript 쪽**이다 — `src/terms.ts:200`(`data.length`), `src/terms.ts:212`(`heldAckBytes += data.length`).

**귀결**: Rust 테스트는 이벤트 payload에서 UTF-16 코드 유닛 수를 **테스트 자신이 계산해** `record_ack`에 넣는 구조가 될 수밖에 없다. 이는 §G가 금지한 "테스트가 ack 수치를 직접 만들어 넣는" 패턴과 형태가 같고(값이 틀린 값일 뿐), 무엇보다 **`terms.ts`가 나중에 `data.length`로 되돌아가도 그 Rust 테스트는 계속 green**이다. R12가 막겠다고 선언한 은폐 메커니즘이 TS 쪽에 그대로 남는다.

**환경 제약 확인**: TS 테스트 러너가 프로젝트에 없다(`package.json` scripts = `dev / build / preview / tauri`, vitest·jest 설정 파일 및 `*.test.ts` 0건). 그리고 `spec.md §C 의존성` 제약이 "신규 npm 의존성 추가 없음"을 규정하므로, 테스트 러너 도입은 현행 제약 아래서 선택지가 아니다. 즉 이 갭은 **제약이 만든 구조적 갭**이며, SPEC이 이를 인지·명문화하지 않은 것이 결함이다.

**필수 수정**: (a) R12의 적용 범위를 "백엔드 emit 회계 경로"로 한정 서술하고, TS 배선의 유일한 기계 가드가 AC-4/AC-5 grep + AC-10a~d 실기기 autotest임을 §D.3 잔여 위험에 명시한다. (b) AC-10a~d의 판정 근거를 강화한다 — 비ASCII 홍수 표본에서 `acked / emitted` 비율이 0.9 이상(정상 반사 ack) 임을 추가 판정하면 UTF-16 회귀를 수치로 잡아낸다. 현행 판정 4건은 밸브가 도입된 뒤에는 **밸브가 10초마다 회계를 리셋해도 전부 true**가 될 수 있어 결함을 잡지 못한다(F5-b: 밸브가 종단 가드를 무력화하는 경로 — `plan.md:194`가 우려한 "느린 팬 위장"이 autotest 판정에서 실제로 성립한다).

---

### F6 — `plan.md:174~178` (M2), `acceptance.md:81` (AC-13) — Severity: MINOR — Class: blocking

**관측**: bench의 Phase A는 **정확히 10초 동안 ack를 전혀 합성하지 않으면서** `pump_once`를 50ms마다 구동한다(`src-tauri/src/bin/bench.rs:307~329`: `phase_a_deadline = phase_a_start + Duration::from_secs(10)`, 루프 내 `ack_output` 호출 없음). bench는 축소 `FlowConfig`를 주입하지 않으므로 `stall_timeout`은 기본 10초(`flow_state.rs:31`)다.

**위험**: M2의 emitter 밸브가 도입되면 Phase A의 무ack 구간 길이와 `stall_timeout`이 **같은 값**이 되어, 밸브 발화가 Phase A 종료 경계 부근에서 일어난다(정지 진입이 홍수 시작 직후이므로 발화 시점 ≈ 정지 진입 + 10s). 발화 시 `reset_accounting()`으로 `outstanding → 0`이 되고 방출이 재개되므로 bench가 보고하는 `outstanding_at_end_a`, Phase B 진입 시 ring drop 기준선(`ring_drop_before_b`)이 달라질 수 있다. `flow_ok`(`bench.rs:396`)는 `saw_emitter_paused && saw_outstanding_drained_in_b && oldest_drop_during_ack == 0`이므로 즉시 red가 되지는 않으나, **경계 타이밍 의존**이 새로 생긴다. plan.md 어디에도 이 상호작용 분석이 없다.

**필수 수정**: `plan.md §B`에 리스크 항목을 추가하고 완화책을 확정한다 — bench가 축소가 아닌 **확대** `stall_timeout`(예: 60s)을 주입해 Phase A 구간에서 밸브가 발화하지 않도록 하거나, Phase A를 밸브 발화 관측 구간으로 재정의하고 그 기대 동작을 bench 판정에 명시한다. AC-13의 기준문에도 이 결정을 반영한다.

---

### F7 — `plan.md:48~51` (§A.3 판정 규칙) — Severity: MINOR — Class: blocking

**관측**: 판정 규칙 3개는 (1) ack 진전 시 리셋, (2) 무진전 + `stall_timeout` 경과 시 발화, (3) **정지 → 방출** 전이 시 두 필드 초기화만 규정한다. **방출 → 정지 전이 시 `paused_since = Some(now)` / `paused_at_acked = acked()`를 무장하는 단계가 규칙 목록에 없다.** 필드 주석("정지 진입 시각")은 의도를 시사하나 규칙 본문에는 부재하다.

**위험**: `paused_since`가 `None`인 채 정지 상태에 들어가면 규칙 2가 평가 불가가 되어 밸브가 영구 미발화한다 — 즉 M2 전체가 무효화되는 실패 모드다. 기존 테스트가 `emitter_gate_decision(true)`를 전이 없이 직접 호출하는 사례가 있어(`flow_tests.rs:29`, `flow_tests.rs:38`) 이 초기 상태는 실제로 발생한다.

**필수 수정**: 규칙 0을 추가한다 — "방출 → 정지 전이(반환값 false이고 직전 `was_paused == false`)에서 `paused_since = Some(now)`, `paused_at_acked = acked()`로 무장한다. 정지 상태 진입 시 `paused_since`가 `None`이면 그 tick에 무장하고 발화 판정은 다음 tick부터 수행한다."

---

### F8 — `plan.md:176` (M2 GREEN 산출물), `plan.md:71` (§A.5) — Severity: MINOR — Class: optional

**관측**: §A.5는 autotest가 `ipc.flowStats(pane)` 표본에서 `emitterValveFired`를 수집한다고 규정한다. 그러나 M2 GREEN 산출물 목록은 `flow_state.rs`(카운터 + Rust `FlowStats` 필드)와 `autotest.ts`만 열거하고, **`src/types.ts`의 `FlowStats` 인터페이스(현행 5필드, `src/types.ts:125~131`)에 `emitterValveFired: number`를 추가하는 작업이 누락**되어 있다. 이 누락 상태로는 §A.5의 표본 수집 코드가 `tsc --noEmit`에서 실패한다(AC-12 red).

**필수 수정**: M2 GREEN 산출물에 `src/types.ts` — `FlowStats`에 `emitterValveFired: number` 추가를 명시한다(`plan.md:6`의 변경 파일 목록에는 이미 `src/types.ts`가 포함되어 있으므로 스코프 확장은 아님).

---

### F9 — `acceptance.md:81` (AC-13) — Severity: MINOR — Class: optional

**관측**: 기준문은 "`byteLen` 추가로 인한 **유의미한 성능 회귀가 없다**"인데 검증 방법은 `cargo run --bin bench` → `flow_ok=true` 단일 불리언이다. `flow_ok`는 기능 플래그(`bench.rs:396`)이며 처리량·지연 수치를 담지 않는다. "유의미한"은 기준이 정의되지 않은 판단어이고, 비교 baseline 수치도 지정되어 있지 않다.

**필수 수정**: 성능 축을 판정하려면 bench가 이미 산출하는 처리량 지표를 지정하고 "M1 이전 baseline 대비 X% 이내"로 이진 판정 가능하게 만들거나, AC-13의 기준문을 "bench 흐름 경로 기능 무회귀(`flow_ok=true`)"로 축소하고 성능은 §D.3 잔여 위험으로 이관한다(§C 성능 제약은 O(1) 산출을 요구할 뿐 계측을 요구하지 않으므로 후자가 자연스럽다).

---

### F10 — `acceptance.md:74~79` (AC-9, AC-10a) — Severity: MINOR — Class: optional

**관측**: 본 SPEC의 AC-9("기존 Rust 테스트 스위트 통과")·AC-10a("비ASCII 홍수 ack 전진")가 선행 SPEC-PTY-FLOW-001의 AC-9(ASCII 홍수)·AC-10a(전환-중-부하)와 **동일 ID·상이 의미**로 충돌한다. AC-11 기준문은 "선행 AC-9", "선행 AC-10a"로 구분하고 있으나(`acceptance.md:79`), 회귀 가드 대상과 본 SPEC 기준이 같은 번호를 공유하는 상태는 run 단계 보고에서 오독을 유발한다.

**필수 수정(권고)**: 회귀 가드 참조를 항상 `FLOW-001 AC-9` / `FLOW-001 AC-10a`로 전량 정규화하거나, 본 SPEC의 신규 AC 번호대를 재배치한다.

---

### F11 — `acceptance.md:69` (AC-4) — Severity: MINOR — Class: optional

**관측**: 판정 명령은 `grep -n "data.length" src/terms.ts` → "ack 경로 매치 0건"이다. 그러나 `src/terms.ts:210`의 `view.outBufLen += data.length;`는 IME 버퍼 크기 계산(용량 캡)용 **정당한 잔존 용법**이므로 grep은 항상 최소 1건을 반환한다. 즉 이 AC는 grep 결과에 사람 판단을 요구하며 완전 이진이 아니다.

**필수 수정(권고)**: 판정을 `grep -n "ackBytes\|heldAckBytes" src/terms.ts` 결과에서 `data.length` 유래 값이 0건임으로 재작성하거나, 예상 매치 수를 명시한다(예: "`data.length` 매치는 `outBufLen` 계산 1건만 남는다").

---

### F12 — `acceptance.md:64~83`, `spec.md:14` (tier: M) — Severity: INFO — Class: optional

Tier M의 수용 기준 상한은 16건이다(`.claude/rules/moai/workflow/spec-workflow.md § SPEC Complexity Tier`). AC-ID를 sub-ID 단위로 세면 18건(`progress.md:8` 스스로 "총 18항목"으로 기재), AC-10을 1건으로 세면 15건이다. §D 서두가 sub-ID 규약을 명시하므로 15건 해석이 방어 가능하나, progress.md의 자기 기재(18)와 어긋난다. 표기를 한쪽으로 통일할 것.

---

### F13 — `spec.md:120` (R8 라벨) — Severity: INFO — Class: optional

R8의 패턴 라벨이 `(Event-detected)`로 표기되어 있다. GEARS 정본 패턴명은 `Event-driven`이다. 문장 형태(`When …, the … shall …`)는 정합하므로 MP-2 판정에는 영향이 없으나 라벨은 정정 권고.

---

### F14 — `spec.md §D`, `research.md §B` — Severity: INFO — Class: optional

회계 결손을 만들 수 있는 **또 다른 두 경로**가 증거표·엣지 표에 열거되어 있지 않다. (a) `src/terms.ts:193` — `views.get(paneId)`가 없으면 `writeOutput`이 조기 반환하여, 이미 `record_emit`된 바이트가 영구 미ack로 남는다(팬 마운트 이전/직후 창). (b) `src/terms.ts:313` — `ipc.ackOutput(...).catch(...)`가 IPC 실패를 삼키므로 해당 배치의 ack가 영구 유실된다. 두 경로 모두 R8 밸브가 최종 방어선으로 흡수하는 "미지의 잔여 누수"에 해당하며 §D.3이 일반론으로 인정하고 있으나, **작업 트리에서 이미 식별 가능한 경로**를 "미지"로 남겨둘 이유는 없다. research.md §B 또는 spec.md §D에 알려진 잔여 누수 경로로 등재 권고.

---

### F15 — `acceptance.md §D` — Severity: INFO — Class: optional

§D AC 매트릭스의 각 행은 Given-When-Then 형식이 아니라 기준/검증명령 표 형식이다. GWT 시나리오는 §B(`acceptance.md:13~41`)에 5건 분리 배치되어 있어 검증 레이어 형식 요건 자체는 충족하나, 개별 AC ↔ §B 시나리오의 대응 관계가 명시되어 있지 않다. AC 행에 대응 시나리오 번호를 병기하면 추적성이 개선된다.

---

## 검증된 강점 (증거 인용)

적대적 감사에서도 다음 항목은 **작업 트리 대조로 정확성이 확인**되었다. 이 항목들은 재작업 대상이 아니다.

| 항목 | 확인 결과 |
|---|---|
| research.md §B 증거표 E1~E16 전량 | file:line 16건 전수 대조 — **전부 정확**. `output.rs:92` `record_emit(data.len())` ✓, `terms.ts:200` ✓, `terms.ts:212` ✓, `flow_state.rs:130~134` saturating ✓, `flow_state.rs:171~182` 히스테리시스 ✓, `flow_state.rs:20~21` 워터마크 값 ✓, `flow_state.rs:203`/`275` 밸브 위치 ✓, `session.rs:684~686`/`803~807` R15 리셋 2지점 ✓, `session.rs:936~945` reader park 게이트 ✓, `main.ts:300/309/317` `writeParsedNoAck` ✓, `autotest.ts:273` ASCII 페이로드 ✓, `output.rs:31~38` / `types.ts:229~235` 이벤트 필드 부재 ✓, `output.rs:90~92` 주석-코드 불일치 ✓ |
| **밸브 평가 지점 타당성 (감사 질의 3)** | `output.rs:63~67` — `pump_once`가 `active_sessions_snapshot()`의 `replay_synced` 세션마다 **정지 여부와 무관하게 매 16ms tick `emitter_gate_decision`을 정확히 1회 호출**한다. 정지 상태에서도 게이트가 계속 불리므로 §A.3의 "단일 결정 지점" 전제는 성립하며, 별도 타이머 스레드 없이 밸브가 도달 가능하다. **설계 결함 없음** |
| **R9 타이머 리셋 레이스 (감사 질의 3)** | §A.3은 `paused_at_acked`(스냅샷) vs `acked()`(원자 읽기) 비교로 진전을 판정한다. reader 밸브가 쓰는 `inner.last_ack_at`(`flow_state.rs:156`에서 `record_ack`가 갱신, `flow_state.rs:231`에서 park 루프가 소비)과 **축이 분리**되어 있어 두 밸브의 타이머가 서로 간섭하지 않는다. `record_ack`의 `fetch_add`는 원자이므로 스냅샷 비교에 레이스 없음. **설계 결함 없음** |
| **M1/M2 분리의 유효성 (감사 질의 4)** | M2 밸브는 M1 시점에 존재하지 않으므로 M1 GREEN 판정을 가릴 수 없고, M2 도입 후에도 B4(`plan.md:92`)가 M1 재현 테스트에 큰 `stall_timeout` 주입을 지시하여 마스킹을 차단한다. `FlowConfig` 주입 패턴은 이미 확립되어 있다(`flow_tests.rs:159~217` — `stall_timeout: 80ms` / `400ms` 축소 주입 + 실슬립). **순서 근거 타당** |
| **M1 배선 테스트 실현 가능성** | `flow_tests.rs:335~380`, `flow_tests.rs:384~429`에 `SessionRegistry` + `ring.push()` + `pump_once(&reg, &|ev| …)` 하네스가 이미 존재한다. M1 RED가 요구하는 "`pump_once` 경로 구동 + 이벤트 payload 수령" 테스트는 **신규 인프라 없이 작성 가능**하다 |
| GEARS 요구사항 품질 | R1~R13 전부 5개 GEARS 패턴 중 하나에 정합. 부정 표현이 `shall not`(R5/R6)에만 존재하여 Unwanted 패턴 규율 준수 |
| Out of Scope 규율 | 7개 `### Out of Scope — <topic>` H3 + 각 항목 구체 불릿. 특히 "프론트엔드 독립 바이트 산정 도입"(`spec.md:221~224`)은 기각 사유까지 기록되어 미래 독자의 오인을 차단 |
| depends_on 사전 점검 | `SPEC-PTY-FLOW-001` `status: completed` 확인 — Depends_on Pre-flight 충족 |

---

## 권고 (FAIL 해소 절차)

manager-spec 재위임 시 아래 순서로 처리한다. **F1~F5(blocking)만 해소되면 Testability 0.50 → 1.0, Traceability 0.75 → 1.0으로 회복되어 집계 0.92(조화) 수준이 되며 Tier M 임계값을 통과한다.**

1. **F1 (BLOCKER)** — `acceptance.md:79`(AC-11), `acceptance.md:104`(§D.2), `plan.md:158`(E7)의 `ok:true`를 `flowOk`(또는 신설 집계 필드)로 정정하고, 신규 4체크의 집계 편입 방식을 `plan.md §A.5`에 확정한다. 집계식 수정이 §D PRESERVE/§G 금지의 예외임을 명문화한다.
2. **F2 (MAJOR)** — `acceptance.md:73`(AC-8)에서 존재하지 않는 `valveFired` 필드 전제를 제거하고, `plan.md:64`(§A.4)의 근거 서술을 "내부 카운터(미노출)"로 정정한다.
3. **F3 (MAJOR)** — R6 전용 AC를 신설하거나 AC-4를 확장해 R6을 커버한다. §D.2 DoD의 AC 범위 서술도 함께 갱신한다.
4. **F4 (MAJOR)** — `plan.md:33`의 `byteLen?` 선택 파라미터를 필수화하거나 `{ seq, byteLen }` 묶음으로 바꿔 `plan.md:89`(B1)의 컴파일 시점 가드 주장과 정합시킨다.
5. **F5 (MAJOR)** — R12의 적용 범위를 백엔드로 한정 명시하고, TS 배선의 유일한 기계 가드를 §D.3 잔여 위험에 등재한다. 아울러 AC-10a~d에 `acked/emitted` 비율 판정을 추가해, 밸브가 종단 판정을 통과시켜 결함을 은폐하는 경로를 차단한다.
6. **F6~F8 (MINOR, blocking)** — bench Phase A 10초 구간과 밸브 `stall_timeout` 충돌 분석을 §B 리스크로 추가(F6), §A.3 판정 규칙에 정지 진입 무장 단계 추가(F7), M2 GREEN 산출물에 `src/types.ts` FlowStats 필드 추가 명시(F8).
7. **F9~F15 (optional)** — 오케스트레이터 재량. 반영 시 아티팩트 품질이 개선되나 재감사 통과의 필수 조건은 아니다.

재감사(iteration 2)는 위 열거된 결함 델타에 한정하여 수행한다(Retry Loop Contract). 판정 권한은 plan-auditor에 있으며, 델타 스코프는 재감사 비용을 줄일 뿐 오케스트레이터 자체 판정으로 대체되지 않는다.

---
---

# Round 2 (Iteration 2/3) — 결함 델타 재감사

Iteration: 2/3
Verdict: **PASS**
Overall Score: **0.92** (조화평균) / 0.94 (산술평균) — Tier M PASS 임계값 **0.80** 충족
점수 추이: 0.71 → 0.92 (**상승** — 점수 회귀 없음 ⇒ STOP 신호 미발동)
감사 일자: 2026-08-18

**감사 범위**: Retry Loop Contract에 따라 Round 1이 열거한 결함 델타(F1~F15)의 해소 여부 + 그 수정이 만든 신규 결함에 한정했다. Round 1에서 1.0으로 채점했거나 "설계 결함 없음"으로 확정한 항목(밸브 평가 지점, R9 레이스, M1/M2 순서, research E1~E16)은 재심하지 않았다. 다만 수정 과정에서 **새로 도입된 주장**(B10 bench 주입, AC-16 grep, E17~E22)은 전부 작업 트리에서 직접 검증했다 — 개정 요약이 아니라 아티팩트 본문과 코드를 대조했다.

---

## Must-Pass 재확인

| # | 기준 | 판정 | 비고 |
|---|---|---|---|
| MP-1 | REQ 번호 일관성 | **PASS** | R1~R13 불변(`spec.md:80~156`), 결번·중복 0건 |
| MP-2 | GEARS 형식 (요구사항 레이어) | **PASS** | R8 라벨이 `(Event-driven)`으로 정정됨(`spec.md:120`) — Round 1 F13 해소. 판정은 `spec.md §B`의 `R1~R13` 요구사항 레이어에 대해 수행했고, AC(검증 레이어)에는 GEARS 테스트를 적용하지 않았다 |
| MP-3 | YAML frontmatter | **PASS** | `spec.md:2~15` 무변경, 12필드 + `tier`/`depends_on` 정합 |
| MP-4 | 언어 중립성 | **N/A** | 단일 프로젝트(Rust+TS) 고유 SPEC |
| MP-5 | D7 교차 SPEC | **PASS** | 참조 SPEC 불변(`SPEC-PTY-FLOW-001` = `status: completed`) |
| MP-6 | D8 크로스 플랫폼 | **PASS** | `syscall` 매치 0건 |
| MP-7 | 클래리피케이션 게이트 | **PASS** | `[NEEDS CLARIFICATION]` 미해결 0건(`plan.md:117` / `progress.md:11`은 "없음" 선언 메타 문장) |

---

## 회귀 점검 (Round 1 결함 델타 F1~F15)

| Round 1 ID | 판정 | 검증 증거 (개정 아티팩트 + 코드 대조) |
|---|---|---|
| F1 (BLOCKER — `ok` vs `flowOk`) | **RESOLVED** | 4개 표면 모두 정정 확인: `acceptance.md:87`(AC-11 = `flowOk === true` **AND** `ok === true`, 배제 사유 병기), `acceptance.md:113`(§D.2 DoD 두 필드 병기), `plan.md:201`(E7), `spec.md:156`(R13 "집계 대상 명시 — `report.ok`가 **아니라** `report.flowOk`"). 집계식 확장 예외가 `plan.md:182`(§D PRESERVE 예외)와 `plan.md:243`(§G 단서), `spec.md:166`(§C autotest 집계 행)에 명문화되어 "집계식 수정이 금지인가" 모순도 해소. append-only 규율(기존 5항 문자 그대로 보존)이 `plan.md:105`에 명시 |
| F2 (MAJOR — 존재하지 않는 `valveFired` 필드) | **RESOLVED** | R10이 "신규 필드로 노출"로 재작성되고 현행 5필드 전제가 정정됨(`spec.md:134~136`). `plan.md:77`이 작업 트리 확인 결과("기존 `valveFired` 필드라는 것은 **존재하지 않는다**")를 명시. AC-8 재작성(`acceptance.md:79`) — Rust·TS 양쪽 grep 검증으로 이진화. research에 E19 신설(`research.md:41`). 내가 대조한 코드와 일치: `flow_state.rs:318~324` 5필드, `types.ts:125~131` 5필드, `valve_fired_count()` 호출자 = 테스트 2곳 |
| F3 (MAJOR — R6 미커버) | **RESOLVED** | AC-16 신설(`acceptance.md:92`, 요구사항 열 = R6), §C 엣지 표가 AC-16 (a)(b)(c)로 연결(`acceptance.md:52~53`), M1 검증 목록에 AC-16 추가(`plan.md:214`), DoD에 명시(`acceptance.md:108`). R1~R13 전량 AC 대응 성립 |
| F4 (MAJOR — 선택 파라미터 vs B1 주장) | **RESOLVED** | `writeOutput(paneId, data, meta?: { seq; byteLen })` 객체 묶음으로 확정(`plan.md:34~44`), 선택 파라미터 기각 사유가 내가 지적한 실패 경로(`terms.ts:253`의 `if (ackBytes > 0)` false → 조용한 무ack)를 그대로 인용해 기록됨. B1 완화책 재작성(`plan.md:125`), AC-12가 컴파일 오류 승격을 단언(`acceptance.md:88`). **코드 대조로 실효성 확인**: 현행 호출부 `terms.writeOutput(paneId, ev.data, ev.seq)`(`main.ts:337`, `main.ts:814`)는 `meta`에 `number`를 넘기므로 미수정 시 타입 오류가 되고, 배너 호출부(`main.ts:348`, `main.ts:826`)는 2인자라 그대로 통과한다 — 완화책이 실제로 성립 |
| F5 (MAJOR — TS 배선 무가드 + 밸브 마스킹) | **RESOLVED** | R12에 "적용 범위 한정 (구조적 갭 명시)" 단락 추가(`spec.md:148`), §C에 "테스트 인프라" 제약 행 신설(`spec.md:167`), §D.3 **최우선** 잔여 위험으로 등재 + 두 축(grep 트립와이어 / 실기기 autotest) 명시(`acceptance.md:118~120`), research E21 신설(`research.md:43` — 내가 확인한 `package.json` scripts 4종과 일치). 밸브 마스킹 차단은 AC-10e(`acked/emitted >= 0.9`) + AC-10f(`emitterValveFired` 증가량 0)로 이진화(`acceptance.md:85~86`), 근거가 R13(`spec.md:154`)과 `plan.md:103`에 서술 — "(a)~(d)만으로는 밸브가 결함을 통과시킨다"는 내 지적을 정면으로 수용 |
| F6 (MINOR — bench Phase A ↔ `stall_timeout` 경계) | **부분 해소 — R2-F2 참조** | 리스크 등재·완화책 선언·파일 목록 추가는 모두 확인(`plan.md:134` B10, `plan.md:6~7`, `plan.md:222`, `acceptance.md:89` AC-13, `research.md:44` E22). 그러나 **완화책의 전제(bench가 `FlowConfig`를 주입할 수 있다)가 작업 트리에서 반증된다** — 아래 R2-F2 |
| F7 (MINOR — `paused_since` 무장 단계 누락) | **RESOLVED** | 규칙 0 신설(`plan.md:59~64`) — 전이 경로(i)와 무전이 직접 호출 경로(ii)를 모두 덮고, (ii)가 실제로 발생하는 근거로 `flow_tests.rs:22-45`를 인용(내가 Round 1에서 든 `flow_tests.rs:29/38`과 동일 지점). 발화 시 무장 해제(규칙 2)·방출 시 초기화(규칙 3)까지 명시. AC-6에 "정지 진입 tick(무장 tick)에는 발화하지 않는다" 추가(`acceptance.md:77`), §G 안티패턴에도 등재(`plan.md:248`) |
| F8 (MINOR — types.ts FlowStats 누락) | **RESOLVED** | M2 GREEN에 굵게 명시(`plan.md:221`) + 누락 시 AC-12 red가 된다는 근거까지 기재 |
| F9 (MINOR — AC-13 성능 주장) | **RESOLVED** | AC-13이 "**기능 무회귀**"로 재작성(`acceptance.md:89`), 성능 미계측이 §D.3 잔여 위험으로 이관(`acceptance.md:124`) — "`flow_ok`는 기능 플래그이지 성능 지표가 아니다"까지 명시 |
| F10 (MINOR — AC ID 충돌) | **RESOLVED** | AC-ID 네임스페이스 규약 신설(`acceptance.md:66`), 선행 SPEC 참조가 전부 `FLOW-001 AC-N`으로 접두 표기됨(`acceptance.md:87`, `plan.md:92`, `plan.md:177`) |
| F11 (MINOR — AC-4 grep 판단 개입) | **RESOLVED** | AC-4가 "매치 수 **정확히 1건**(`outBufLen` 계산)"으로 이진화(`acceptance.md:75`), E6도 동일하게 정정(`plan.md:200`) |
| F12 (INFO — AC 계수 표기) | **RESOLVED** | sub-ID 규약 + "논리 AC 16건(AC-1~AC-16), AC-10의 6 sub-ID는 1건 계수" 명시(`acceptance.md:68`) — Tier M 상한 16과 정합 |
| F13 (INFO — R8 라벨) | **RESOLVED** | `(Event-driven)`으로 정정(`spec.md:120`) |
| F14 (INFO — 알려진 누수 미등재) | **RESOLVED** | research E17/E18 신설(`research.md:39~40` — `terms.ts:193` 조기 반환, `terms.ts:313` `.catch` 삼킴, 내가 지적한 두 지점과 정확히 일치), spec §D 2행 추가(`spec.md:197~198`), plan B11/B12(`plan.md:135~136`), acceptance §C 2행 + §D.3(`acceptance.md:54~55`, `:123`). "수정하지 않고 등재만 한다"는 범위 경계도 명시 |
| F15 (INFO — AC↔시나리오 미연결) | **RESOLVED** | AC 매트릭스에 "시나리오" 열 신설(`acceptance.md:70~92`) |

**해소율**: 15건 중 14건 완전 해소, 1건(F6) 전제 반증으로 부분 해소. **3회 연속 미해결 결함(정체) 0건** — 정체 신호 없음.

---

## 카테고리 점수 (Round 2)

| 차원 | Round 1 | Round 2 | 밴드 | 증거 |
|---|---|---|---|---|
| Clarity | 0.75 | **1.0** | 1.0 | 감점 사유 2건이 모두 제거됨 — 시그니처가 실제 TS 코드로 확정(`plan.md:37`)되고 기각 사유까지 기록(`plan.md:44`), 밸브 판정이 규칙 0~3의 완결 집합으로 서술(`plan.md:59~64`). 요구사항 R1~R13은 단일 해석만 허용 |
| Completeness | 1.0 | **1.0** | 1.0 | 전 섹션 + frontmatter 12필드 + Out of Scope H3 7건 유지. R12 적용 범위 한정 단락, §C 테스트 인프라 제약 행, §D.3 5개 잔여 위험으로 오히려 보강 |
| Testability | 0.50 | **0.75** | 0.75 | 대폭 개선 — AC-11이 올바른 집계 필드를 지목(`acceptance.md:87`), AC-8이 실재하는 대상으로 재작성, AC-10e/f가 밸브 마스킹을 수치로 차단, AC-12가 컴파일 오류를 단언, AC-4가 정확 매치 수로 이진화. 1.0을 주지 않는 이유는 잔여 2건 — AC-16(a)의 grep 기대값이 정상 코드에서 어긋나고(R2-F1), AC-13이 현 코드베이스에서 달성 불가한 전제에 걸려 있다(R2-F2) |
| Traceability | 0.75 | **1.0** | 1.0 | R1~R13 전량 AC 대응(R6 → AC-16). 고아 AC 0건 — AC-14/AC-15는 요구사항이 아닌 §C 제약을 참조한다고 열에 명시. 시나리오 열로 §B↔§D 연결까지 확보 |

집계: 조화평균 4 / (1/1.0 + 1/1.0 + 1/0.75 + 1/1.0) = **0.923**, 산술평균 **0.9375**. Tier M 임계값 0.80 충족 → **PASS**.

점수 추이 0.71 → 0.92는 **상승**이므로 LEAN 워크플로의 점수 회귀 STOP 조항은 발동하지 않는다.

---

## 잔여 결함 (Round 2 신규)

아래 2건(R2-F1/R2-F2)은 **이번 수정이 새로 만든** 결함이며, PASS 판정과 별개로 run 단계 진입 전(또는 해당 마일스톤 착수 시) 정정을 권고한다. PASS는 이 결함들을 게이트하지 않는다 — must-pass 전항 통과 + 집계 0.92이므로 판정은 PASS이되, 결함은 은폐 없이 열거한다.

### R2-F1 — `acceptance.md:92` (AC-16 (a)) — Severity: **MAJOR** — Class: blocking

**주장**: `grep -c "writeParsedNoAck" src/main.ts` → **5건**(스냅샷 1 + replay 3 + exit 배너 1).

**반증(실행 관측)**: 해당 명령의 실제 출력은 **6**이다. `grep -c`는 **줄 수**를 세는데, `src/main.ts:316`이 호출이 아니라 주석(`// 완료 시 전진 → writeParsedNoAck 콜백이 replay.lastSeq 로 갱신`)이면서 문자열을 포함하기 때문이다. 실제 호출부는 5곳이다 — `main.ts:300`(스냅샷), `:309`(replay 오버플로 배너), `:317`(replay 데이터), `:324`(exit 배너), `:328`(no session 배너).

**귀결**: AC-16은 MUST이고 R6의 유일한 AC다. 코드가 **정확히 올바른 상태**여도 이 판정은 FAIL로 나온다 — Round 1 F11(주석 줄 때문에 grep 수가 어긋나는 결함)과 정확히 같은 클래스가 신규 AC에 재유입되었다. AC-4는 같은 함정을 "정확히 1건 + 그 1건이 `outBufLen` 행"으로 이미 해결했으므로, 동일 규율이 AC-16에는 적용되지 않은 비대칭이다.

**필수 수정**: 판정 명령을 호출부만 매칭하도록 좁힌다 — `grep -c "terms.writeParsedNoAck(" src/main.ts` → **5**(주석 줄은 `terms.` 접두와 `(`가 없어 제외된다). 아울러 내역 표기를 관측된 호출부와 일치시킨다: 스냅샷 1(`:300`) + replay 경로 3(`:309` 오버플로 배너 / `:317` replay 데이터 / `:324` exit 배너) + catch 배너 1(`:328` no session). 현행 "exit 배너 1"이 `:328`(no session)을 가리키는지 `:324`(exit)를 가리키는지 모호하므로 줄 번호를 병기할 것.

### R2-F2 — `plan.md:134` (B10), `plan.md:6~7`, `plan.md:222`, `acceptance.md:89` (AC-13) — Severity: **MAJOR** — Class: blocking

**주장**: "bench가 `FlowConfig`에 **확대** `stall_timeout`(60s)을 주입하여 Phase A 구간에서 emitter 밸브가 발화하지 않게 한다"(B10). 변경 규모는 "`FlowConfig` 주입 1줄이며 스코프 확장이 아니다"(`plan.md:7`).

**반증(작업 트리 관측)**: registry가 생성하는 세션에는 **`FlowConfig` 주입 경로가 존재하지 않는다**.
- `src-tauri/src/session.rs:522`와 `src-tauri/src/session.rs:1217` — 두 생성 지점 모두 `flow_state: crate::flow_state::FlowState::default()`로 하드코딩되어 있다.
- `with_config` 호출자를 전수 검색한 결과 `flow_state.rs:102`(default 경유)와 테스트뿐이며, 프로덕션·bench 경로에는 없다.
- 결정적으로 **bench 자신이 이 불가능성을 이미 주석으로 기록해 두었다** — `src-tauri/src/bin/bench.rs:263~271`: *"M1 이 spawn_session 에 config 주입 경로를 제공하지 않았다(`FlowState::with_config` 는 standalone 인스턴스에만 사용 가능, spawned 세션은 `Arc<PtySession>` 배후라 config 필드 수정 불가)"*. `FlowState.config`가 `pub`이더라도 `Arc<PtySession>` 뒤에서는 내부 가변성 없이 변경할 수 없다.

**귀결**: (a) B10의 완화책은 현 코드베이스에서 **1줄로 구현 불가**하며, 구현하려면 `session.rs`에 주입 seam(생성 파라미터 / 설정 필드 / 환경변수 오버라이드)을 새로 뚫어야 한다. (b) `session.rs`는 `plan.md:6`의 7개 변경 파일 목록에 **없다** — seam을 뚫는 순간 `plan.md:7`의 "스코프 확장이 아니다"가 거짓이 된다. (c) AC-13이 그 완화책을 기준문에 못박았으므로(`acceptance.md:89`) **AC-13은 현재 명세대로는 달성 불가**다. (d) 따라서 Round 1 F6이 지적한 위험(Phase A의 정확히 10초 무ack 구간 ↔ 기본 `stall_timeout` 10초의 경계 충돌)은 **실질적으로 미완화 상태로 남는다**. research E22는 `bench.rs:307-329`만 인용하고, 40줄 위의 반대 증거(`bench.rs:263~271`)는 채록하지 않았다.

**필수 수정 — 아래 넷 중 하나를 명시적으로 채택한다**:
1. **환경변수 오버라이드**(권장 — 최소 침습): `FlowConfig::default()`가 `stall_timeout`을 환경변수(예: `TERMF_FLOW_STALL_TIMEOUT_MS`)로 덮어쓸 수 있게 하고 bench가 그 값을 설정한다. `flow_state.rs` 1곳 변경으로 끝나며 `session.rs` seam이 불필요하고, 신규 **임계값 상수**가 아니라 기존 값의 테스트 오버라이드이므로 §C 상수 규율과도 충돌하지 않는다.
2. **bench Phase A 구간 단축**: `phase_a_deadline`을 10초 → 8초로 낮춰 무ack 구간이 기본 `stall_timeout` 아래에 머물게 한다. `bench.rs` 1줄. 측정 창이 바뀌므로 기존 표본과의 비교 가능성은 별도 판단 필요.
3. **밸브 발화를 수용하고 명시 판정**: Phase A에서 밸브가 1회 발화함을 기대 동작으로 인정하고 AC-13을 그에 맞게 재작성한다(`saw_emitter_paused`가 여전히 true임 + 발화 횟수 ≤ 1).
4. **`session.rs` 주입 seam 신설**: 채택 시 `plan.md:6` 변경 파일 목록에 `session.rs`를 추가하고, `plan.md:7`의 "스코프 확장 아님" 문장을 철회하며, §D PRESERVE에 대한 예외를 F1과 같은 방식으로 명문화할 것.

어느 쪽을 택하든 B10 본문·`plan.md:7`·AC-13 기준문·research E22를 함께 정정하고, E22에 `bench.rs:263~271`의 반대 증거를 채록한다.

### R2-F3 — `plan.md:222` (M2 GREEN bench.rs 산출물) — Severity: MINOR — Class: blocking

AC-13의 판정 기준에 "표본 `emitter_valve_fired == 0`"이 포함되었으나(`acceptance.md:89`), M2 GREEN의 `bench.rs` 산출물은 "`FlowConfig`에 `stall_timeout: 60s` 주입"만 명시한다. bench의 표본 JSON은 손으로 조립되므로(`src-tauri/src/bin/bench.rs:318~327`의 Phase A, `:358~366`의 Phase B) `emitter_valve_fired` 필드를 추가하는 작업이 별도로 필요하다. Round 1 F8(types.ts 누락)과 동일 클래스의 산출물 목록 누락이다. **필수 수정**: M2 GREEN의 `bench.rs` 항목에 표본 JSON 필드 추가를 명시한다.

### R2-F4 — `plan.md:42` (§A.2 호출부 목록), `acceptance.md:92` (AC-16 (b)) — Severity: MINOR — Class: optional

`terms.writeOutput(` 호출부가 `main.ts` 밖에도 **2곳 더 존재**한다 — `src/autotest.ts:607`, `src/autotest.ts:623`(IME 체크의 합성 마커). 둘 다 2인자 호출이므로 `meta?` 선택 형태에서 컴파일은 통과하고 ack 대상도 아니어서(R6 정합) 동작상 문제는 없다. 그러나 §A.2의 호출부 인벤토리와 §F 변경 파일 목록 어디에도 등재되지 않았고, AC-16 (b)의 grep이 `src/main.ts`로만 한정되어 이 2곳은 어떤 AC의 시야에도 들어오지 않는다. **권고**: §A.2에 "autotest.ts 2곳은 `meta` 미전달 유지(합성 마커, ack 대상 아님)"를 1줄 추가하거나, AC-16 (b)의 grep 범위를 `src/`로 확대한다.

### R2-F5 — `plan.md:255` (§H 참조) — Severity: INFO — Class: optional

§H가 여전히 "`acceptance.md` — AC-1~AC-15"로 기재되어 있다. AC-16 신설 후 갱신되지 않은 잔여 표기. `AC-1~AC-16`으로 정정 권고.

### R2-F6 — `acceptance.md:68`, `acceptance.md:75` (AC-4) — Severity: INFO — Class: optional

두 가지 운영 주의:
1. 논리 AC가 **정확히 16건**으로 Tier M 상한(16)과 같다. AC를 하나라도 더 추가하면 상한을 넘겨 tier-up 또는 SPEC 분할 신호가 된다 — 위 R2-F1/R2-F3 정정은 기존 AC의 기준문 수정이므로 AC 수를 늘리지 않는 방식으로 처리할 것.
2. AC-4의 "정확히 1건"은 현행 코드에서 **4건**(`terms.ts:199` 주석 / `:200` / `:210` / `:212`)이므로, 200·212 제거뿐 아니라 **`:199` 주석("data.length 만큼 ack 누적")도 함께 갱신해야** 성립한다. 이는 결함이 아니라 AC가 의도적으로 강제하는 정리 작업이며, 구현자가 "주석은 안 고쳐도 되겠지"로 넘어가면 AC-4가 FAIL로 나온다는 점을 M1 착수 시 인지할 것.

---

## PASS 판정 근거 (must-pass별 증거)

- **MP-1**: `spec.md:80~156` — R1~R13 순차·무중복. 개정에서 요구사항 번호 변동 없음.
- **MP-2**: `spec.md:120` R8 = `(Event-driven)` + `When … the session flow-control state shall …`; R1/R2/R10/R11/R12/R13 Ubiquitous, R3/R9 Event-driven, R4/R7 State-driven, R5/R6 Unwanted(`shall not`). **판정 레이어 명시**: 이 판정은 `spec.md §B`의 `REQ` 레이어에 대해서만 수행했으며, `acceptance.md §B`의 Given-When-Then 시나리오와 §D의 AC 행은 검증 레이어이므로 GEARS 테스트 대상에서 제외했다.
- **MP-3**: `spec.md:2~15` — 12필드 전량 + 타입 정합, 거부 별칭 0건.
- **MP-4**: N/A(단일 프로젝트 고유 SPEC, 템플릿 바인딩 다국어 표면 아님).
- **MP-5**: `.moai/specs/SPEC-PTY-FLOW-001/spec.md:5` = `status: completed` → D7 BLOCKING 없음.
- **MP-6**: `syscall` 매치 0건 → 자동 PASS.
- **MP-7**: `[NEEDS CLARIFICATION]` 미해결 0건.

집계 0.92(조화) / 0.94(산술)로 Tier M 임계값 0.80을 충족하고, must-pass 전항이 통과했으며, 점수 회귀도 없다. 따라서 **PASS**로 판정한다. R2-F1·R2-F2는 blocking 클래스로 열거하되 판정을 뒤집지 않으며, 오케스트레이터는 run 단계 진입 전(R2-F1은 M1 AC-16 검증 전, R2-F2는 M2 bench 작업 착수 전)에 해당 정정을 라우팅할 것을 권고한다. R2-F4~F6은 optional로 오케스트레이터 재량이다.
