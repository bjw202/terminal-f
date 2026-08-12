---
id: SPEC-PTY-FLOW-001
title: "PTY 출력 흐름 제어 (ack-watermark flow control) + 워크스페이스 전환 출력 유실 수정"
version: "0.2.1"
status: in-progress
created: 2026-08-12
updated: 2026-08-12
author: manager-spec
priority: P0
phase: "v0.1.3 target"
module: "src-tauri/src, src"
lifecycle: spec-anchored
tags: "pty, flow-control, backpressure, xterm, ring-buffer, replay, ime, conpty"
tier: M
---

# SPEC-PTY-FLOW-001 — PTY 출력 흐름 제어 + 전환 출력 유실 수정

## HISTORY

| 버전 | 날짜 | 작성자 | 변경 내용 |
|---|---|---|---|
| 0.1.0 | 2026-08-12 | manager-spec | 최초 작성 (plan-phase 아티팩트 생성). 결함 2건(프론트엔드 방향 흐름 제어 부재, 워크스페이스 전환 시 출력 유실)에 대한 ack-watermark 해법(사용자 승인 Plan A) 명세 |
| 0.2.0 | 2026-08-12 | manager-spec | plan-audit iteration 1(0.75 FAIL) 대응. D1 flow 회계 리셋(R15) 신설 — 좌초 outstanding에 의한 emitter 영구 정지 차단, D4 teardown disarm 선행 규율(R8 보강), D5 replay–emitter seq 경합 금지(R16) 신설, D6 R6 패턴 라벨 정정, D7 R12/R13 이중 부정 제거, D8 `ACK_FLUSH_IDLE_MS` 상수화, D9 `flow_stats` 관측 커맨드(R1 보강) |
| 0.2.1 | 2026-08-12 | manager-spec | plan-audit iteration 2(0.93 PASS) 후 자문 사항 정리. N1 늦은 ack 순서 규정(R15 — 전환 전 ack 플러시 완료, epoch 태깅은 대안으로 기각 기록), N2 replay 진행 중 이벤트 지연 적용 규정(R10), N3 plan.md §H R 개수 표기 정정 |

---

## §A 배경과 목적

### A.1 결함 1 — 프론트엔드 방향 흐름 제어 부재 (보고된 "Claude Code 팬 점진적 프리즈" 버그의 근본 원인)

백엔드 emitter(`src-tauri/src/output.rs`)는 xterm.js가 이전 데이터를 파싱했는지와 무관하게 `EMIT_INTERVAL_MS=16` 주기로 무조건 방출한다. `term.write()`의 내부 큐는 **무한**(unbounded)이다. 특정 팬의 프로그램(예: Claude Code)이 그 팬의 파싱+렌더 처리량보다 빠른 출력을 지속하면 — 특히 WebGL 컨텍스트 유실 후 DOM 렌더러로 강등된 상태에서 — 웹뷰 안에서 백로그가 무한히 자란다. 지연은 ms → 초 → 분 단위로 상승하고, 해당 팬은 입력 에코와 스크롤이 죽어 프리즈된 것처럼 보인다(다른 팬은 정상). ring의 oldest-drop(ADR-004)은 **백엔드 메모리만** 보호하며, 웹뷰 측 백로그는 보호하지 못한다.

### A.2 결함 2 — 워크스페이스 전환 시 출력 유실

`src/main.ts`의 `pty-output` 핸들러는 이벤트 **수신 즉시** `view.lastSeq = ev.seq`를 전진시키지만, 데이터는 여전히 xterm write 큐 그리고/또는 IME 보류 버퍼에 남아 있을 수 있다. `snapshotAndDispose`는 (fire-and-forget) 플러시 직후 즉시 `serialize()`하므로, 큐에 있으나 미파싱된 데이터는 스냅샷에 없는데도 `lastSeq`는 소비했다고 주장한다 — 이후 `replay_pane(lastSeq)`가 그 구간을 영구히 건너뛴다. 백로그가 클수록 구멍도 커진다.

### A.3 채택된 해법 방향 (사용자 승인 — VS Code / xterm.js 업계 표준 패턴)

ack 기반 워터마크 흐름 제어로 OS 수준 블로킹 사슬을 IPC 경계 너머로 연장한다: 프론트엔드가 **파싱을 마친** 바이트를 `ack_output`으로 보고하고, 백엔드는 미확인(outstanding) 바이트가 워터마크를 넘으면 방출을 멈춘다. 활성 세션의 ring이 임계치를 넘으면 reader 스레드가 read를 멈춰 ConPTY 파이프가 차오르게 하고, 자식 프로세스의 write()가 표준 터미널 동작대로 블로킹된다 — 활성 팬의 데이터는 드랍되지 않는다. 같은 기계장치의 parsed-seq 회계가 결함 2를 함께 고친다. ADR-004의 불변식("느린 UI가 백엔드 메모리를 키울 수 없다")은 유지된다 — reader는 여전히 UI 완료를 기다리며 블로킹되지 않고, condvar park는 ring 점유량이라는 백엔드 자체 상태에만 의존한다.

---

## §B 요구사항 (GEARS)

GEARS 구조 키워드(`Where` / `While` / `When` / `shall`)와 코드 식별자는 영문 정본을 유지하고, 서술부는 한국어로 기술한다.

### 명명 상수 (seed 값 — 최종 값 근거는 plan.md §A.1)

| 상수 | seed 값 | 위치 |
|---|---|---|
| `FLOW_HIGH_WATERMARK` | 128 KiB (131072) | `src-tauri/src/output.rs` |
| `FLOW_LOW_WATERMARK` | 32 KiB (32768) | `src-tauri/src/output.rs` |
| `RING_PAUSE_THRESHOLD` | 768 KiB (`RING_MAX_BYTES`의 75%) | `src-tauri/src/session.rs` |
| `FLOW_STALL_TIMEOUT` | 10 s | `src-tauri/src/session.rs` |
| `READER_PARK_RECHECK_MS` | 100 ms | `src-tauri/src/session.rs` |
| `ACK_BATCH_BYTES` | 4 KiB | `src/terms.ts` |
| `ACK_FLUSH_IDLE_MS` | 50 ms | `src/terms.ts` |
| `SNAPSHOT_DRAIN_TIMEOUT_MS` | 500 ms | `src/terms.ts` |

### R1 — 세션별 흐름 제어 회계 (Ubiquitous)

> The session flow-control state **shall** 누적 `emitted_bytes: AtomicU64`와 `acked_bytes: AtomicU64`를 세션별로 유지하고, `outstanding = emitted_bytes.saturating_sub(acked_bytes)`로 미확인 바이트를 산출한다.

`emitted_bytes`는 emitter가 `pty-output` 이벤트를 방출하는 시점에 방출된 payload 바이트 수만큼 전진한다. 누적 u64이므로 실사용 범위에서 래핑하지 않으며, 방어적으로 saturating 연산만 사용한다.

관측 가능성: the session registry **shall** 디버그 커맨드 `flow_stats(pane_id)`를 제공하여 `{emitted, acked, outstanding, emitter_paused, reader_parked}`를 반환한다 — autotest/bench가 워터마크 경계(AC-9)를 기계 판정하는 데 쓴다. 백엔드 원자 변수는 프론트 주도 autotest에서 직접 관측할 수 없으므로 이 커맨드가 유일한 관측 창구다.

### R2 — ack 수신 (Event-driven)

> **When** 프론트엔드가 `ack_output(pane_id, bytes)` Tauri 커맨드를 호출하면, the session registry **shall** 해당 팬의 세션 `acked_bytes`에 `bytes`를 가산한다.

존재하지 않는 pane/세션에 대한 ack는 오류 없이 조용히 무시한다(전환·teardown 경합에서 정상적으로 발생 가능).

### R3 — emitter 워터마크 게이트, 히스테리시스 (State-driven)

> **While** 세션의 `outstanding > FLOW_HIGH_WATERMARK`이면, the emitter (`pump_once`) **shall** 그 세션의 방출을 건너뛴다(`last_emitted_seq` 미전진, 데이터는 ring에 누적). **When** 일시정지 상태에서 `outstanding <= FLOW_LOW_WATERMARK`에 도달하면, the emitter **shall** 방출을 재개한다.

HIGH/LOW 이중 워터마크는 히스테리시스다: 일시정지 후 LOW 이하로 내려올 때까지 재개하지 않아 경계에서의 진동(flapping)을 막는다. 게이트는 세션별 독립이며, 한 팬의 정체가 다른 팬의 방출을 막지 않는다.

### R4 — reader 게이트: 활성 세션 ring 임계 시 park (State-driven)

> **While** 세션이 live-emitting 모드(`replay_synced == true`)이고 ring의 미방출(un-emitted) 바이트가 `RING_PAUSE_THRESHOLD`를 초과하면, the reader thread **shall** 각 `reader.read()` 호출 **이전에** condvar에 park하고(`READER_PARK_RECHECK_MS` 주기 재확인), 데이터를 읽지도 드랍하지도 않는다.

읽기를 멈추면 ConPTY 파이프가 차오르고 자식 프로세스 자신의 write()가 블로킹된다 — 표준 터미널 동작이며, 활성 팬에 대해 어떤 데이터도 유실되지 않는다.

### R5 — live 모드 이탈 시 게이트 해제 (Event-driven)

> **When** 세션이 live 모드를 벗어나면(워크스페이스 전환으로 `replay_synced = false`, 또는 teardown), the reader gate **shall** 즉시 해제(disarm)되고, 오늘의 비활성 워크스페이스 의미론(ring 누적 + oldest-drop)이 **변경 없이** 적용된다.

### R6 — 정지 안전밸브 (Event-driven)

> **When** reader 게이트가 park된 채 `FLOW_STALL_TIMEOUT` 동안 ack 진전이 전혀 감지되지 않으면(프론트엔드 사망/웨지), the reader thread **shall** 읽기를 재개하고 오늘의 oldest-drop + 기존 overflow 배너 경로로 폴백한다.

자식 프로세스가 깨진 UI 때문에 영원히 웨지되어서는 안 된다. 밸브 발화는 R15의 flow 회계 리셋을 **동반**한다 — 리셋으로 emitter 게이트(R3)가 해제되어야 oldest-drop 경로의 overflow 배너가 실제로 도달 가능하다(리셋 없이는 emitter가 잠긴 채라 폴백 경로 자체가 막힌다).

### R7 — flow-paused 세션은 주입 idle 게이트에서 BUSY (State-driven)

> **While** 세션이 flow-paused(reader park) 상태이면, the injection idle gate (`SessionRegistry::inject`의 `require_idle`) **shall** 그 세션을 BUSY로 취급한다.

park 중에는 `last_output_at`이 갱신되지 않아 홍수(flood) 와중에 거짓 "idle" 판정이 나는 것을 막는다.

### R8 — teardown 시 즉시 join (Event-driven)

> **When** `teardown_session`이 호출되면, the session registry **shall** signal **이전에** 게이트 해제(disarm) 플래그를 설정한 뒤 park condvar를 signal하여, reader 스레드가 지체 없이 join되게 한다(기존 teardown 계약 유지).

disarm 선행이 필수인 이유: teardown은 `replay_synced`를 지우지 않으므로, disarm 없이 깨어난 reader는 ring이 여전히 임계 초과인 한 park 조건을 재평가하고 **영원히 재park**한다(join 교착). 깨어난 reader는 park 조건 재평가 전에 disarm을 먼저 검사한다. 상세는 plan.md §A.4.

### R9 — 파싱 완료 시 ack + parsedSeq 전진 (Event-driven)

> **When** PTY 출력 경로의 `term.write(data, cb)` 콜백이 실행되면(xterm 파싱 완료), the frontend **shall** (a) ack 바이트를 누적하여 누적치가 `ACK_BATCH_BYTES` 이상이거나 마지막 콜백 후 `ACK_FLUSH_IDLE_MS` 경과 시 `ack_output`을 배치 플러시하고, (b) 그 팬의 `parsedSeq`를 전진시킨다.

작은 write마다 IPC 1회를 호출하지 않는다 — 배치가 요구사항이다.

### R10 — seq 이원화: receivedSeq vs parsedSeq (Ubiquitous)

> The pane view **shall** `receivedSeq`(이벤트 수신 시점 전진)와 `parsedSeq`(write 콜백에서 전진)를 분리 유지하고, 마운트 시 `replay_pane` 호출과 스냅샷에 저장되는 `lastSeq`는 **`parsedSeq`** 를 사용한다.

`src/main.ts`의 `pty-output` 핸들러는 더 이상 `lastSeq`를 직접 전진시키지 않는다. 또한 the frontend **shall** `replay_pane` invoke가 진행 중인 팬으로 향하는 `pty-output` 이벤트를 지연 버퍼링하여 replay 데이터 적용 **이후에** 순서대로 적용하고, dispose된 팬으로 향하는 이벤트는 파싱도 ack도 하지 않고 폐기한다 — remount 전후의 출력 순서 연속성이 유지된다.

### R11 — 스냅샷 전 드레인 (Event-driven)

> **When** `snapshotAndDispose(paneId)`가 실행되면, the frontend **shall** IME 보류 버퍼를 플러시한 뒤 미완료 write 콜백을 `SNAPSHOT_DRAIN_TIMEOUT_MS` 한도로 대기(드레인)하고 나서 `serialize()`한다. **When** 드레인이 타임아웃되면, the frontend **shall** 그대로 serialize하되 정확성은 유지된다 — `parsedSeq`는 실제 파싱된 범위만 가리키므로 remount 후 `replay_pane(parsedSeq)`가 공백을 채운다.

### R12 — IME 보류 데이터 미ack (Unwanted)

> The frontend **shall not** IME 보류 버퍼(`imeBuffering`)에 머물러 아직 `term.write`에 도달하지 않은 데이터를 ack한다.

(금지 규정 — 부정은 `shall not`에만 있다.) 보류 데이터의 ack는 해당 chunk가 최종적으로 `term.write`에 도달한 뒤 그 write의 콜백에서만 발생한다.

ack은 오직 write 콜백에서만 발생한다(R9의 자연 귀결이나, 명시적 요구사항으로 고정한다).

### R13 — replay 데이터 미ack (Unwanted)

> The frontend **shall not** `replay_pane` 커맨드 응답으로 수신한 데이터를 `ack_output`으로 보고한다.

(금지 규정 — 부정은 `shall not`에만 있다.)

byte 회계는 live emit 경로(`pty-output` 이벤트) 전용이다 — replay 바이트를 ack하면 `acked > emitted` 왜곡이 생긴다. 단 `parsedSeq`는 replay 데이터의 파싱 완료 시에도 전진한다(seq 회계와 byte 회계는 별개 축). 백엔드는 방어적으로 saturating_sub를 사용해 어떤 ack 초과에도 패닉/래핑하지 않는다.

### R14 — 무회귀 (Ubiquitous)

> The app **shall** 비활성 워크스페이스의 ring/replay 의미론, injection 게이트 API 표면, automation engine, control-pipe API를 변경하지 않으며, 기존 테스트 스위트(Rust 단위 테스트 ~92건, autotest 32 체크)를 전부 통과한다.

### R15 — flow 회계 리셋 (Event-driven)

> **When** 다음 세 가지 중 하나가 발생하면 — (i) `replay_synced = false` 전이(워크스페이스 이탈), (ii) remount 시 `replay()` 재무장(re-arm), (iii) 정지 밸브(R6) 발화 — the session flow-control state **shall** 미확인 회계를 리셋한다(`acked_bytes := emitted_bytes`, 즉 `outstanding = 0`).

리셋이 없으면 un-acked 바이트가 세 시나리오에서 영구 좌초된다: (a) 전환 시 xterm dispose로 pending write 콜백이 소멸하고 R13이 remount 후 replay 데이터 ack를 금지하므로 그 구간은 영원히 ack되지 않는다, (b) 프론트엔드 사망/웨지, (c) 드레인 타임아웃. 좌초된 `outstanding > FLOW_HIGH_WATERMARK`는 R3의 emitter 정지를 영구화한다(방출 없음 → ack 없음 → 재개 없음). 리셋 후 내용 정합성은 byte 회계가 아니라 seq 회계가 보증한다 — `parsedSeq` 기반 replay(R10)가 연속성을 담보하므로, byte 회계는 "현재 live 구간의 배압 신호"로만 유효하면 충분하다.

늦은 ack 순서 규정: 리셋 **이후에** 백엔드에 도착하는 옛 뷰의 ack 배치는 `acked_bytes`를 영구 부풀린다. 이를 막기 위해 the frontend **shall** 워크스페이스 전환(dispose 경로)을 발동하기 **전에** 잔여 ack 배치의 플러시 invoke 완료를 await한다 — 백엔드 도착 순서가 "마지막 ack → 리셋"으로 고정된다. (대안으로 검토·기각: ack에 flow epoch/generation을 태깅하고 백엔드가 stale-epoch ack를 드랍하는 방식 — 세션별 상태가 하나 늘고 검증 표면이 커져 채택하지 않음.)

### R16 — replay()–emitter seq 경합 금지 (Ubiquitous)

> The session registry and emitter **shall** `last_emitted_seq` / `replay_synced`에 대한 `replay()`와 `pump_once`의 동시 접근을 상호 배제하여 seq 되감김(rewind)을 금지한다 — emitter는 replay가 저장한 더 새로운 `last_seq`를 자신의 낡은 값으로 덮어쓰지 않으며, replay된 구간은 live 방출로 중복 재방출되지 않는다.

무보호 교차의 사고 시나리오: pump가 옛 `last_emitted_seq`를 로드 → replay가 더 새로운 `last_seq` 저장 + 재무장 → pump가 자신의 낡은 `last_seq`를 저장 → seq 되감김 → replay된 범위가 live로 중복 재방출. 메커니즘 결정은 plan.md §A.9 (replay의 collect+store를 emitter 배치와 동일한 ring 락 범위에서 수행; 대안: replay generation counter).

---

## §C 제약 (Constraints)

| 구분 | 제약 |
|---|---|
| 아키텍처 불변식 | ADR-004 유지 — "느린 UI가 백엔드 메모리를 키울 수 없다". reader는 UI 완료 콜백을 기다리며 블로킹되지 않는다. park 조건은 ring 점유량(백엔드 자체 상태)에만 의존하고, 정지 안전밸브(R6)가 최종 방어선이다. |
| 스레딩/락 | 락 순서 역전 금지(기존 store → registry 순서 준수). condvar용 뮤텍스는 세션별 flow 상태에 국한하고 registry 전역 락을 쥔 채 park하지 않는다. teardown 경로는 반드시 condvar signal을 포함한다(R8). |
| 의존성 | 신규 crate/npm 의존성 추가 없음. `portable-pty`, `xterm.js 5.5` 현행 유지. |
| 개발 방법론 | TDD (RED → GREEN → REFACTOR). `quality.yaml constitution.development_mode: tdd`. |
| 커버리지 | 신규 Rust 순수 로직(워터마크 판정, park/unpark 조건, 정지 밸브, ack 회계) 단위 테스트 필수. 목표 85%, 커밋당 최소 80%. |
| autotest 함정 | autotest(`TERMF_AUTOTEST=1`)는 앱을 자체 종료시킨다. **terminal-f 팬 안에서 절대 실행 금지**, 리포트 파일이 정본(scratchpad에 쓰고 밖에서 읽는다). |
| **문서화 의무** | `docs/DEVELOPMENT.md` §9 — 기능당 ADR 1건 신규 작성 + 동반 문서 갱신. 다음 빈 번호는 **ADR-014**다(ADR-001~013 존재). 흐름 제어 설계(워터마크 근거, reader park, 정지 밸브, seq 이원화)를 ADR-014로 기록하고 `docs/DEVELOPMENT.md`·`docs/ARCHITECTURE.md`를 갱신한다. 산출은 sync 단계 책임이며 검증 기준은 acceptance.md AC-12. |
| 문서 언어 | 모든 문서는 한국어(코드 식별자·파일명·명령어는 영어 유지). ADR-014도 한국어로 쓴다. |
| @MX 태그 | reader 게이트와 정지 밸브에 `@MX:WARN`(+`@MX:REASON`), 워터마크 상수와 ack 회계 불변식에 `@MX:ANCHOR` 후보. `code_comments: ko` — 태그 설명은 한국어. |
| 상수 규율 | 모든 임계값은 명명 상수(§B 표)로만 도입한다. 매직 넘버 금지. seed 값 조정은 허용되나 근거를 plan.md §A.1에 기록한다. |

---

## §D 부록 — 엣지 케이스

| 케이스 | 기대 동작 |
|---|---|
| WebGL 유실 → DOM 렌더러 강등으로 파싱 처리량 급감 | 본 SPEC의 핵심 시나리오. outstanding이 HIGH를 넘으면 방출 정지 → ring 누적 → ring이 임계를 넘으면 reader park → 자식 write() 블로킹. 데이터 유실 없음. WebGL 재획득 자체는 §E 범위 제외 |
| 프론트엔드 사망/웨지 (webview 크래시 등) | R6 정지 밸브: `FLOW_STALL_TIMEOUT` 후 읽기 재개 + oldest-drop + 기존 overflow 배너. 밸브 발화 시 R15 회계 리셋으로 emitter 게이트(R3)가 풀려 배너 경로가 실제로 도달 가능. 자식 프로세스는 영구 웨지되지 않음 |
| park 중 워크스페이스 전환 | R5: `replay_synced=false` 설정 즉시 게이트 해제, reader 재개, 비활성 의미론(누적+oldest-drop) 그대로. R15: 전이 시 byte 회계 리셋(`acked := emitted`) — 좌초 outstanding이 남지 않아 remount + replay 재무장 후 live 방출이 즉시 재개된다 |
| replay()와 pump_once 동시 실행 | R16: 상호 배제로 seq 되감김·replay 구간 중복 재방출 없음 (plan §A.9) |
| park 중 teardown | R8: condvar signal → reader 스레드 즉시 join. 기존 `teardown_session` 계약 유지 |
| 이미 dispose된 팬에 대한 늦은 ack | R2: 조용히 무시. 오류·로그 스팸 없음 |
| ack 초과 (`acked > emitted`) | R13 방어: saturating_sub로 outstanding은 0으로 바닥. 패닉/래핑 없음 |
| replay로 받은 데이터의 회계 | byte ack 없음(R13), parsedSeq는 전진. replay는 emitter 회계를 거치지 않으므로 대칭 유지 |
| IME 조합 중 홍수 | 보류 버퍼(`imeBuffering`) 데이터는 write 도달 전이므로 미ack(R12). 보류분은 outstanding에 남아 백엔드 게이트가 자연히 조여짐 — 조합 중 백로그 폭주 방지 |
| 여러 팬 동시 홍수 | 회계·게이트 모두 세션별 독립(R1/R3). 느린 팬 하나가 다른 팬의 방출을 막지 않음 |
| park 중 injection (`require_idle`) | R7: BUSY 판정. `last_output_at`이 정체됐다는 이유로 홍수 중 주입되는 사고 방지 |
| 드레인 타임아웃 (500ms 내 파싱 미완) | R11: 즉시 serialize. parsedSeq가 미파싱 구간을 가리키지 않으므로 remount 후 replay가 공백을 채움 — 유실 대신 재생 |
| outstanding이 LOW와 HIGH 사이 | 히스테리시스(R3): 직전 상태 유지 — 방출 중이었으면 계속 방출, 정지 중이었으면 LOW 이하까지 정지 유지 |

---

## §E 범위 제외 (Exclusions)

본 SPEC이 **의도적으로 만들지 않는** 것들이다. 미래의 독자가 "버그"로 오인하지 않도록 명시한다.

### Out of Scope — WebGL 컨텍스트 유실 재획득

- WebGL 컨텍스트 유실 시 재획득(re-acquisition) 시도는 구현하지 않는다. 유실 후 DOM 렌더러 강등 상태는 그대로 둔다.
- 본 SPEC은 강등 상태에서도 백로그가 유한하게 유지되도록 만들 뿐이다. 재획득은 별도 후속 SPEC.

### Out of Scope — IME 에코 패스스루 윈도우 강화

- `lastInputTs`가 xterm 자동 응답으로 갱신되는 문제 등 IME 에코 패스스루 윈도우(`ECHO_PASS_MS`) 강화는 별도 후속 SPEC.
- 본 SPEC의 IME 접점은 R12(보류 데이터 미ack)와 R11(스냅샷 전 드레인)로 한정한다.

### Out of Scope — 비활성 워크스페이스 ring/replay 의미론 변경

- 비활성 워크스페이스의 ring 누적 + oldest-drop + `replay_pane` 재생 의미론은 변경하지 않는다(R5/R14).
- reader 게이트는 live-emitting 세션에만 무장(arm)된다. 비활성 세션에서 자식 write()를 블로킹하지 않는다 — 비활성 팬을 위해 자식을 멈추는 것은 의도적으로 하지 않는다.

### Out of Scope — injection 게이트·automation engine·control-pipe API 변경

- `SessionRegistry::inject`의 API 표면, automation rule engine, named-pipe control API는 변경하지 않는다.
- R7은 idle 판정 입력에 flow-paused 상태를 추가할 뿐, API·프로토콜은 그대로다.

### Out of Scope — 워터마크의 문자 기반 산정

- VS Code는 문자(char) 기반 100KB/5KB 워터마크를 쓰지만, 본 SPEC은 16ms 병합 배치 위의 **바이트 기반** 워터마크를 채택한다. 문자 수 산정(파서 개입)은 도입하지 않는다.

### Out of Scope — ring 용량·emit 주기 상수 변경

- `RING_MAX_BYTES`(1MiB), `RING_MAX_CHUNKS`(1024), `EMIT_INTERVAL_MS`(16), `READ_CHUNK_SIZE`(8192)는 변경하지 않는다. 신규 상수만 추가한다.

---

## §F 참조

- `.moai/specs/SPEC-PTY-FLOW-001/plan.md` — 구현 계획, 마일스톤, 기술 결정
- `.moai/specs/SPEC-PTY-FLOW-001/acceptance.md` — 수용 기준, Given-When-Then
- `docs/ADR-004-backpressure-ring-buffer.md` — 유지되는 불변식("느린 UI가 백엔드 메모리를 키울 수 없다")의 정본
- `docs/ADR-005-live-pty-memory-policy.md` — live PTY 메모리 정책 (ring 상수의 배경)
- `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md` — sync 단계에서 갱신 (ADR-014 신규)
