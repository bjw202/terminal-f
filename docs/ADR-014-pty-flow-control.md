# ADR-014: PTY 출력 흐름 제어 (ack-watermark flow control)

## 배경

### 결함 1: 프론트엔드 방향 흐름 제어 부재

Claude Code 팬에서 활성 워크스페이스의 프로그램(예: Claude Code 자신)이 대량 출력을 지속적으로 방출할 때, 백엔드 emitter(`src-tauri/src/output.rs`)는 xterm.js가 이전 데이터를 파싱했는지와 무관하게 `EMIT_INTERVAL_MS=16` 주기로 무조건 방출했다. xterm.js의 내부 큐는 **무한**(unbounded)이었다.

특히 WebGL 컨텍스트 유실 후 DOM 렌더러로 강등된 상태에서는 파싱 처리량이 급격히 감소하여, 웹뷰 안에서 백로그가 무한히 자라났다. 지연은 ms → 초 → 분 단위로 상승했고, 해당 팬은 입력 에코와 스크롤이 죽은 것처럼 보이는 **프리즈 현상**이 발생했다.

기존의 ring buffer oldest-drop(ADR-004)은 **백엔드 메모리만** 보호할 뿐, 웹뷰 측 백로그는 보호하지 못했다.

### 결함 2: 워크스페이스 전환 시 출력 유실

`src/main.ts`의 `pty-output` 핸들러는 이벤트 **수신 즉시** `view.lastSeq = ev.seq`를 전진시켰으나, 데이터는 여전히 xterm write 큐 그리고/또는 IME 보류 버퍼에 남아 있을 수 있었다. `snapshotAndDispose`는 (fire-and-forget) 플러시 직후 즉시 `serialize()`했으므로, 큐에 있으나 미파싱된 데이터는 스냅샷에 없는데도 `lastSeq`는 소비했다고 주장했다 — 이후 `replay_pane(lastSeq)`가 그 구간을 영구히 건너뛰었다. 백로그가 클수록 구멍도 커졌다.

## 결정

ack 기반 워터마크 흐름 제어로 OS 수준 블로킹 사슬을 IPC 경계 너머로 연장한다.

### 핵심 아키텍처

1. **ack-watermark R3 히스테리시스**: 프론트엔드가 **파싱을 마친** 바이트를 `ack_output`으로 보고하고, 백엔드는 미확인(outstanding) 바이트가 워터마크를 넘으면 방출을 멈춘다. **회계 단위는 UTF-8 바이트이며 백엔드가 단일 원천이다** — emit 측은 `PtyOutputEvent.byteLen`(배너 포함 최종 문자열의 바이트 길이, `output.rs`의 `byte_len`)을, ack 측은 프론트엔드가 그 값을 **반사**(재산정 없이 그대로)하여 보고한다(SPEC-PTY-FLOW-002 R2/R5). 프론트엔드의 UTF-16 코드 유닛 수 기반 재산정은 금지된다(단위 불일치 → outstanding 영구 잔존 → 출력 정지).

2. **reader park R4**: 활성 세션의 ring이 `RING_PAUSE_THRESHOLD`를 넘으면 reader 스레드가 read를 멈춰 ConPTY 파이프를 차오르게 하고, 자식 프로세스의 write()가 표준 터미널 동작대로 블로킹된다.

3. **정지 밸브 R6 (reader 측)**: `FLOW_STALL_TIMEOUT`(10s) 동안 ack 진전이 전혀 없으면(프론트 사망/웨지), reader는 읽기를 재개하고 oldest-drop + 기존 overflow 배너 경로로 폴백한다.

3a. **emitter 정지 안전밸브 (SPEC-PTY-FLOW-002 R8)**: reader 밸브 R6와 나란히, **emitter 게이트 자체에도** 독립 안전밸브가 있다. emitter가 정지 상태이고 ack이 `stall_timeout` 동안 무진전이면 밸브가 발화하여 회계를 리셋하고 방출을 재개한다(`flow_state.rs` `emitter_gate_decision`의 판정 규칙, `emitter_valve_fired` 카운터로 관측). 이 밸브는 **최종 방어선**이지 1차 해법이 아니다 — 단위가 어긋나도 10초마다 복구되어 결함이 "느린 팬"으로 위장될 수 있으므로, 발화가 관측되면 단위 회계 결함을 의심해야 한다. `TERMF_FLOW_STALL_TIMEOUT_MS` 환경변수로 타임아웃을 오버라이드할 수 있다(테스트·bench 주입용, 미설정 시 10s 불변).

4. **parsedSeq 이원화 R10**: `receivedSeq`(이벤트 수신 시점 전진)와 `parsedSeq`(write 콜백에서 전진, **정본**)를 분리하여, `replay_pane(paneId, parsedSeq)`와 `snapshot.lastSeq`가 정본 seq를 사용하도록 한다.

5. **회계 리셋 R15**: 3지점에서 `acked_bytes := emitted_bytes`로 리셋 — (i) `replay_synced=false` 전이, (ii) `replay()` 재무장, (iii) 정지 밸브 발화. 좌초된 outstanding이 emitter 게이트를 영구 잠그는 것을 막는다.

6. **R16 동일 ring 락 범위**: `replay()`의 collect+store와 `pump_once`의 collect+store가 동일한 ring 락 범위에서 수행되어 seq 되감김을 금지한다.

## 워터마크 근거

### seed 값

- **FLOW_HIGH_WATERMARK 128KiB**: 16ms당 최대 방출량은 reader 처리량에 좌우되나, 한 배치가 수십 KiB에 달할 수 있다(8KiB read × 여러 회 병합). HIGH가 너무 낮으면 정상 부하에서도 게이트가 진동한다. 128KiB ≈ 정상 파싱 속도에서 수십 ms 분량의 백로그 — 체감 지연 없이 폭주만 차단하는 수준이다.

- **FLOW_LOW_WATERMARK 32KiB**: VS Code의 5KB보다 높게 잡는다. 우리 ack은 4KiB 배치라서 LOW가 너무 낮으면 재개 시점이 ack 배치 입도(granularity)에 걸려 늦어진다. HIGH의 25%로 히스테리시스 폭을 확보한다.

- **RING_PAUSE_THRESHOLD 768KiB**: `RING_MAX_BYTES`(1MiB)의 75% — UI가 방출을 따라가지 못해 ring이 찰 때 reader가 park 하여 ConPTY 파이프를 차오르게 하고 자식 write()를 블로킹한다.

- **FLOW_STALL_TIMEOUT 10s**: "느린 프론트"와 "죽은 프론트"를 구분하는 유일한 신호는 ack 진전이다. 10s 동안 진전이 없으면 프론트 사망으로 판정한다.

- **READER_PARK_RECHECK_MS 100ms**: condvar signal이 누락되더라도 100ms 내 재평가하여 안전망이다.

- **ACK_BATCH_BYTES 4KiB**: 작은 write마다 IPC 1회 금지 — 배치 규칙이다.

- **ACK_FLUSH_IDLE_MS 50ms**: emit 주기(16ms)의 약 3배 — 잔여 ack이 LOW 워터마크 재개 판정을 체감할 만큼 지연시키지 않을 정도로 짧고, idle-flush IPC를 초당 ~20회로 상한할 정도로 길다.

- **SNAPSHOT_DRAIN_TIMEOUT_MS 500ms**: 드레인은 "재생량 최소화" 최적화이지 정확성 조건이 아니다 — `parsedSeq`가 파싱된 범위만 가리키므로 remount 후 replay가 공백을 채운다.

### bench/autotest 계측

- **autotest maxOutstanding 32,665**: 실기기 autotest flood 시나리오에서 `flow_stats`가 관측한 최대 outstanding 값.
- **bench soak**: 극단 부하에서 emitter 정지·reader park 발동이 관측되었다.
- **switch-under-load**: `progressed: true`, `noGap: true` — 전환 경계 내용 공백 없음 (결함 2 회귀 확정 차단).

seed 값은 개발 기기 계측 기반이며, 저사양 기기에서의 최적값은 다를 수 있다. 상수화되어 있으므로 후속 조정 비용은 낮다.

## 트레이드오프

### vs oldest-drop 단독

oldest-drop 단독(ADR-004)은 백엔드 메모리만 보호하고 웹뷰 측 백로그는 보호하지 못한다. ack-watermark는 OS 수준 블로킹 사슬을 IPC 경계 너머로 연장하여, **활성 팬의 데이터는 유실시키지 않으면서** 백엔드 메모리와 웹뷰 측 백로그를 함께 보호한다.

### vs VS Code 문자 기반 워터마크

VS Code는 문자(char) 기반 100KB/5KB 워터마크를 쓰지만, 본 프로젝트는 16ms 병합 배치 위의 **바이트 기반** 워터마크를 채택한다. 문자 수 산정(파서 개입)은 도입하지 않는다. 바이트 기반은 구현이 단순하고, ack 배치로 IPC 빈도를 효율적으로 제어한다.

## 테스트 구조

### Rust 단위 테스트

- **워터마크 게이트 순수 로직**: skip(>HIGH)·resume(<=LOW)·히스테리시스 유지
- **ack 회계**: u64 누적, saturating 방어, 미지 세션 무시
- **reader 게이트 park/unpark 조건**: live+임계 초과 시 park, disarm 조건 즉시 해제
- **정지 밸브**: 무ack 10s 후 재개+폴백, ack 진전 시 타이머 리셋
- **teardown-under-park join**: ring이 `RING_PAUSE_THRESHOLD` 초과를 유지한 상태에서 disarm → signal → join
- **flow-paused 세션 BUSY 판정**: park 중 `last_output_at` 정체로 거짓 "idle" 판정 방지
- **flow 회계 리셋 3지점**: 전이/재무장/밸브 발화 각각에서 `outstanding == 0`
- **replay()–pump_once 동시 실행**: seq 되감김·중복 재방출 없음 (R16)

### autotest

- **flood**: pwsh 루프로 대량 출력 생성 → ack 진전 확인, `outstanding <= HIGH_WATERMARK + 1배치` 유지, 활성 팬에 "[output overflow]" 배너 없음, 최종 tail 정상
- **switch-under-load**: 홍수 중 워크스페이스 이탈→복귀, 스냅샷/replay 경계 내용 공백 없음 (결함 2 회귀)

### bench

- **soak**: reader park 발생·해제 관측, 활성 세션 ring 무한 성장 없음, ack 흐름 중 oldest-drop 없음. ack 진행을 합성하고 워터마크/타임아웃을 주입한다.

## R4 fix 비고

### 결함

`check_reader_park_gate`가 미방출 **청크 수**(seq 차이, 최대 ~1024)를 `ring_pause_threshold`(**바이트**, 768KiB)와 비교하여 reader park가 실제로 발동하지 않았다(R4 MUST 무력화 — 활성 팬 홍수 시 자식 write() 블로킹이 안 돼 oldest-drop 유실 경로로 폴백).

### 수정

- `RingBuffer::un_emitted_bytes(from_seq)` 추가
- `check_reader_park_gate` + 신규 비블로킹 관측 helper `reader_should_park_now`를 바이트 기준으로 전환
- RED→GREEN: 재현 테스트 `ac_3_reader_park_gate_uses_bytes_not_chunks`(청크 1개 + 임계 초과 바이트 → true) — RED 확정 후 수정으로 GREEN

### 종단 검증

bench 재실행 시 `reader_parked false→true` 전이 관측(outstandingEndA ~814KiB > 768KiB), `flow_ok=true`, `oldest_drop_during_ack=0`. 135 tests green, clippy NEW 0.

## SPEC-PTY-FLOW-002 개정 — ack 단위 불일치 결함과 그 해소

### 결함 3: ack 단위 불일치로 인한 출력 영구 정지

FLOW-001 시점의 프론트엔드는 `ack_output`에 **UTF-16 코드 유닛 수**를 산정해 보고했으나, 백엔드 emit 회계는 **UTF-8 바이트**였다. 한글 등 non-ASCII 출력에서 UTF-16 유닛 수 < UTF-8 바이트 수이므로 ack이 emit을 영원히 따라잡지 못했고, outstanding이 0으로 수렴하지 않아 emitter 게이트가 재개 조건(`≤ LOW 32KiB`)을 만족하지 못했다. 증상: 대량 non-ASCII 출력 후 해당 팬 출력이 영구 정지(전환 시 `reset_accounting`으로 일시 복구되는 것처럼 보여 결함을 은폐).

**사각지대**: outstanding > 32KiB(게이트 잠금 지속)인데 ring 미방출분 < 768KiB면 reader park(R4)도 발동하지 않았다 — 두 밸브 사이의 간극에서는 어느 안전망도 결함을 잡지 못했다. 이 간극이 SPEC-PTY-FLOW-002의 발견 계기다.

### 수정 (M1 — 단위 통일)

- **백엔드 단일 원천**: `PtyOutputEvent.byteLen`(`output.rs` `byte_len`, 배너 포함 최종 문자열 기준)이 ack 수치의 유일한 원천.
- **반사 ack**: `terms.ts`는 이벤트의 `byteLen`을 재산정 없이 그대로 ack 누적. `seq` 없는 배너·replay/스냅샷 데이터는 ack하지 않는다(`writeParsedNoAck` 경로, `ackBytes = 0`).
- **재현-우선 회귀 테스트**: `flow_tests.rs` `flow002_ac2_utf16_unit_repro_permanent_pause`(RED → GREEN).

### 수정 (M2 — emitter 정지 안전밸브)

위 §핵심 아키텍처 3a의 emitter 밸브 + `TERMF_FLOW_STALL_TIMEOUT_MS` env 오버라이드 + `FlowStats`의 `valve_fired`/`emitter_valve_fired` 관측 필드(bench 표본 JSON 포함). 정상 경로에서는 `emitter_valve_fired == 0`이어야 한다(bench 판정).

### 알려진 잔여 누수 (밸브로 흡수, 기명 부채)

회계에서 ack이 누락될 수 있는 경로 2곳이 남아 있고, 둘 다 emitter 밸브가 10s 내 흡수한다(plan §B B11/B12 등록):

- **(a) 마운트 해제된 pane의 early-return**: `pty-output` 핸들러가 해당 pane을 찾지 못하면 ack 없이 반환 — outstanding이 남지만 밸브가 리셋.
- **(b) ack IPC 실패 삼킴**: 프론트의 `ack_output` invoke 실패가 로그만 남고 회계에는 반영 안 됨 — 동일하게 밸브가 흡수.

이들은 데이터 유실을 일으키지 않는(emit이 이미 완료된 데이터의 회계만 어긋나는) 누수이며, 밸브가 최종 방어선으로 개입한다.

### 검증 (SPEC-PTY-FLOW-002)

- Rust: 148 tests green(단위 통일 + 밸브 발화/미발화/관측 + `flow002_*` 재현 테스트군), clippy NEW 0.
- autotest: u8 flood 6판정(u8FloodAckProgress, u8FloodOutstandingBounded ≤ 512KiB, u8FloodTailRendered 등) + 기존 flow 5판정 전부 true.
- bench: `flow_ok=true`, 전 표본 `emitter_valve_fired == 0`.

## 참조

- **ADR-004** (유지 불변식): "느린 UI가 백엔드 메모리를 키울 수 없다" — reader는 여전히 UI 완료를 기다리며 블로킹되지 않는다. park 조건은 ring 점유량(백엔드 자체 상태)에만 의존한다.
- **ADR-005** (live PTY 메모리 정책): ring 상수의 배경
- **VS Code terminal flow control** (업계 표준 패턴 출처; 문자 기반 100KB/5KB → 본 SPEC은 바이트 기반)

---

**버전**: 1.1.0 (SPEC-PTY-FLOW-002 개정: 회계 단위 명시 + emitter 안전밸브 + 잔여 누수 기명)  
**날짜**: 2026-08-19 (초판 2026-08-12)  
**SPEC**: SPEC-PTY-FLOW-001, SPEC-PTY-FLOW-002
