# SPEC-PTY-FLOW-001 — 구현 계획 (plan)

> Tier: **M** (standard) — 백엔드+프론트엔드 약 7개 파일(`session.rs`, `output.rs`, `commands.rs`, `lib.rs`, `terms.ts`, `main.ts`, `ipc.ts`) + 테스트/autotest/bench. 신규 의존성 없음. LOC 추정 300~700. Tier M 3-파일 아티팩트 세트(spec/plan/acceptance) 적용.
>
> 섹션 순서는 결정-가역성(decision-reversibility) 기준이다: 바뀔 가능성이 높은 결정(§A 상수·인터페이스·UX 흐름)을 앞에, 기계적 작업(§F 마일스톤의 배선 단계)을 뒤에 둔다. 설계 방향(ack-watermark)은 사용자 승인 완료 — 재론하지 않는다. seed 상수는 근거와 함께 조정 가능하되 아키텍처 교체는 불가.

---

## §A 기술 결정 (가역성 높은 순)

### A.1 워터마크 seed 값 근거 — `FLOW_HIGH_WATERMARK` 128KiB / `FLOW_LOW_WATERMARK` 32KiB

가장 조정 가능성이 높은 결정. VS Code는 문자 기반 100KB(high)/5KB(low)를 쓴다. 본 프로젝트는 16ms 병합 배치 위의 **바이트 기반**이므로 다르게 잡는다:

- **HIGH 128KiB**: 16ms당 최대 방출량은 reader 처리량에 좌우되나, 한 배치가 수십 KiB에 달할 수 있다(8KiB read × 여러 회 병합). HIGH가 너무 낮으면 정상 부하에서도 게이트가 진동한다. 128KiB ≈ 정상 파싱 속도에서 수십 ms 분량의 백로그 — 체감 지연 없이 폭주만 차단하는 수준.
- **LOW 32KiB**: VS Code의 5KB보다 높게 잡는다. 우리 ack은 4KiB 배치라서 LOW가 너무 낮으면 재개 시점이 ack 배치 입도(granularity)에 걸려 늦어진다. HIGH의 25%로 히스테리시스 폭을 확보.
- run 단계에서 flood autotest/bench 계측으로 조정 가능. **조정 시 ADR-014에 최종 값과 계측 근거를 기록**한다.

### A.2 seq 이원화 — `receivedSeq` vs `parsedSeq` (뷰 타입 인터페이스 변경)

- 팬 뷰 객체의 `lastSeq` 단일 필드를 `receivedSeq`(이벤트 수신 시 전진, 진단용)와 `parsedSeq`(write 콜백에서 전진, **정본**)로 분리한다.
- `replay_pane(paneId, fromSeq)`의 `fromSeq`와 `snapshotAndDispose`가 저장하는 `lastSeq`는 `parsedSeq`만 사용한다. `main.ts`의 `pty-output` 핸들러는 seq를 직접 전진시키지 않는다(수신 기록만).
- 이벤트 하나(병합 배치)는 seq 하나 + payload 바이트 N을 갖는다. write 콜백은 배치 단위로 발화하므로 `parsedSeq`는 "이 배치까지 파싱 완료"로 전진 — 배치 내부 부분 파싱 상태는 없다(xterm write 콜백은 해당 write 전체 파싱 후 호출).

### A.3 ack IPC 형태 — `ack_output(pane_id, bytes)` + 배치

- 커맨드 시그니처는 `ack_output(pane_id: String, bytes: u64)`. 프론트는 pane 단위로 말하고, 백엔드 registry가 pane→session 매핑을 푼다(기존 커맨드 관례와 일치).
- 배치 규칙: write 콜백에서 누적 → 누적치 ≥ `ACK_BATCH_BYTES`(4KiB) 즉시 플러시, 또는 마지막 콜백 후 `ACK_FLUSH_IDLE_MS`(seed 50ms) 경과 시 잔여분 플러시. 작은 write마다 IPC 1회 금지.
- `ACK_FLUSH_IDLE_MS` 50ms 근거: emit 주기(16ms)의 약 3배 — 잔여 ack이 LOW 워터마크 재개 판정을 체감할 만큼 지연시키지 않을 정도로 짧고, idle-flush IPC를 초당 ~20회로 상한할 정도로 길다. 상수 규율(spec §C) 준수 — 인라인 리터럴 금지.
- 미지의 pane ack는 조용히 무시(전환/teardown 경합의 정상 경로).

### A.4 reader 게이트 설계 — 세션별 `Mutex<FlowState> + Condvar`

- park 판단은 `reader.read()` 호출 **이전**: `replay_synced == true && !disarmed && ring_unemitted_bytes > RING_PAUSE_THRESHOLD`이면 `Condvar::wait_timeout(READER_PARK_RECHECK_MS)` 루프.
- **disarm 규율 (교착 방지)**: `teardown_session`은 condvar signal **이전에** disarm 플래그를 설정한다(또는 lifecycle `Closing` 상태 검사로 갈음). teardown은 `replay_synced`를 지우지 **않으므로**, disarm 조건이 predicate에 없으면 깨어난 reader가 ring 임계 초과를 재확인하고 영원히 재park하여 join이 교착한다. 깨어난 reader는 park 조건 재평가 전에 disarm을 먼저 검사한다. (spec R8, AC-5)
- **회계 리셋 접점 (R15)**: `replay_synced=false` 전이와 `replay()` 재무장 지점에서 `acked_bytes := emitted_bytes`로 리셋한다. 전환으로 좌초된 outstanding(콜백 소멸 + replay 미ack)이 emitter 게이트를 영구 잠그는 것을 막는 핵심 — remount 후 live 방출 재개는 AC-14가 단위 테스트로 고정한다.
- 깨우는 신호: (a) ack 수신으로 outstanding 감소(emitter가 방출 후 ring이 줄어듦), (b) `replay_synced=false` 전이, (c) teardown(disarm 선행). 어느 경우든 100ms 주기 재확인이 안전망.
- **락 규율**: condvar 뮤텍스는 세션별 flow 상태만 감싼다. registry 전역 락을 쥔 채 park 금지(교착 방지). 기존 store → registry 락 순서 불변.
- `@MX:WARN` + `@MX:REASON`(ko) 부착 대상: park 루프, 정지 밸브. `@MX:ANCHOR` 대상: 워터마크 상수 블록, outstanding 산출식.

### A.5 정지 안전밸브 — `FLOW_STALL_TIMEOUT` 10s

- park 진입 시점의 `acked_bytes`를 기억하고, 10s 경과에도 값이 그대로면 프론트 사망으로 판정 → park 해제, 오늘의 oldest-drop + overflow 배너 경로로 폴백.
- ack가 조금이라도 진전되면 타이머 리셋. "느린 프론트"와 "죽은 프론트"를 구분하는 유일한 신호는 ack 진전이다.
- **밸브 발화 = 회계 리셋 동반 (R15)**: 발화 시 `acked_bytes := emitted_bytes`. 리셋 없이는 `outstanding > HIGH`가 굳어 emitter 게이트(R3)가 잠긴 채라, 폴백이 약속한 overflow 배너(pump_once 경유)가 **도달 불가능**하다. 리셋이 폴백 경로의 전제 조건이다.

### A.6 스냅샷 전 드레인 — 500ms 한도

- `snapshotAndDispose`: IME 플러시 → 미완료 write 콜백 Promise들을 `Promise.race([all, timeout(500ms)])`로 대기 → `serialize()`.
- 타임아웃 시에도 정확성 유지: `parsedSeq`가 파싱된 범위만 가리키므로 remount 후 `replay_pane(parsedSeq)`가 공백을 채운다. 드레인은 "재생량 최소화" 최적화이지 정확성 조건이 아니다 — 이 역전이 결함 2 수정의 핵심.

### A.7 사용자 가시 동작 변화

- 활성 팬 홍수 시: 프리즈 대신 자식 프로세스가 자연 감속(write 블로킹). "[output overflow]" 배너는 활성 팬 정상 경로에서 사라지고, 정지 밸브 폴백에서만 나타난다.
- 워크스페이스 전환: 전환 직전 미파싱 백로그가 있어도 복귀 시 내용 공백 없음(replay가 채움). 전환 체감 지연은 최대 500ms 드레인만큼 증가 가능 — 허용 범위로 판단, run 단계에서 체감 확인.

### A.8 미해결 클래리피케이션 없음

설계 방향·seed 상수·마일스톤 모두 사용자 승인 완료. 미해결 결정 없음.

### A.9 replay()–emitter seq 경합 처리 (R16)

`session.rs`의 `replay()`는 collect + `last_emitted_seq` 갱신 + `replay_synced` 재무장을 수행하고, `output.rs`의 `pump_once`는 `last_emitted_seq` 로드 → `ring.collect_since` → 저장을 수행한다. 무보호 교차 시: pump가 옛 `last_emitted_seq` 로드 → replay가 더 새로운 `last_seq` 저장 + 재무장 → pump가 자신의 낡은 `last_seq` 저장 → **seq 되감김** → replay된 범위가 live로 중복 재방출.

- **채택**: replay의 collect+store를 emitter의 배치 collect+store와 **동일한 ring 락 범위**에서 수행하여 check-then-act를 원자화한다. 락 보유 구간은 collect+store로 한정하고 Tauri 이벤트 emit은 락 밖에서 수행(락 보유 중 IPC 금지).
- **기각 대안**: replay generation counter를 emitter가 check-and-skip — 락 범위 통일보다 상태가 하나 늘고 경합 창을 검증하기 어렵다.
- 단위 테스트: AC-15 (교차 실행에서 되감김·중복 재방출 없음).

---

## §B 알려진 리스크 (Known Issues)

- **B.1 교착(deadlock)**: reader park 중 teardown/전환 경로가 condvar signal을 놓치면 join 지연. → R8 요구사항화 + `teardown-under-park` 단위 테스트(AC-5). 락 순서·범위는 §A.4 규율 준수.
- **B.2 replay/ack 회계 비대칭**: replay 바이트를 ack하면 `acked > emitted` 왜곡. → R13(미ack) + saturating_sub 방어 + 단위 테스트(AC-2).
- **B.3 거짓 idle**: park 중 `last_output_at` 정체로 injection 게이트가 홍수 중 "idle" 오판. → R7 + 단위 테스트(AC-6).
- **B.4 autotest 자체 종료 함정**: autotest는 앱을 스스로 끝낸다. terminal-f 팬 안에서 실행하면 세션이 죽는다. **scratchpad에 리포트를 쓰고 밖(포그라운드)에서 읽는다.** 리포트 파일이 정본.
- **B.5 headless pwsh DSR 함정 + bench ack 합성**: bench는 `pump_dsr` 헬퍼로 headless pwsh의 DSR 응답을 처리한다(`bin/bench.rs`). soak 시나리오 확장 시 이 헬퍼를 우회하지 말 것. 또한 headless bench에는 프론트엔드가 없어 ack이 0으로 고정된다 — outstanding이 즉시 HIGH에 도달해 emitter가 정지하고 park/resume 사이클을 관측할 수 없다. 따라서 soak 시나리오는 **ack 진행을 합성**한다(`ack_output` 커맨드 직접 호출 또는 flow 상태 직접 가산). 워터마크·정지 타임아웃 상수는 bench/test 빌드에서 **주입 가능**해야 한다(AC-4의 타임아웃 축소 주입을 일반화).
- **B.6 ack 콜백 순서 가정**: xterm `term.write(data, cb)` 콜백은 write 순서대로 발화한다(xterm.js 계약). IME 보류 버퍼를 거친 chunk도 최종 write 시점의 콜백에서만 ack(R12) — 보류 중 ack 금지.
- **B.7 성능 회귀**: ack IPC·게이트 검사 오버헤드. 배치(4KiB)로 IPC 빈도를 묶고, 게이트 판정은 원자 변수 비교 2회 수준으로 유지. bench로 회귀 확인(AC-11).

---

## §C 사전 점검 (Pre-flight)

run 단계 착수 전 baseline 측정:

```bash
# 1. Rust 테스트 baseline (~92건 green 확인)
cd src-tauri && cargo test 2>&1 | tail -5

# 2. clippy baseline (NEW vs 기존 구분용)
cd src-tauri && cargo clippy 2>&1 | tail -5

# 3. 기존 상수·접점 확인
grep -n "READ_CHUNK_SIZE\|RING_MAX_BYTES\|EMIT_INTERVAL_MS\|replay_synced\|last_emitted_seq" src-tauri/src/session.rs src-tauri/src/output.rs

# 4. autotest baseline (32 체크) — 팬 밖에서 실행, 리포트 파일 확인
# (TERMF_AUTOTEST=1 실행 후 autotest-report.json 판독 — 리포트가 정본)

# 5. 프론트 접점 확인
grep -n "lastSeq\|writeOutput\|snapshotAndDispose\|imeBuffering" src/main.ts src/terms.ts
```

---

## §D 제약 (DO NOT VIOLATE)

- 신규 crate/npm 의존성 금지. `RING_MAX_BYTES`/`RING_MAX_CHUNKS`/`EMIT_INTERVAL_MS`/`READ_CHUNK_SIZE` 변경 금지.
- 비활성 워크스페이스 의미론·injection API·automation engine·control-pipe 무변경 (spec.md §E).
- 매직 넘버 금지 — 모든 임계값은 spec.md §B 명명 상수로만.
- TDD: 각 마일스톤 RED(실패 테스트) 먼저. `--no-verify` 금지.
- autotest를 terminal-f 팬 안에서 실행 금지 (B.4).
- 커밋: Conventional Commits, `feat(SPEC-PTY-FLOW-001): M{N} <subject>` 형식, 커밋 메시지 한국어(`git_commit_messages: ko`).

---

## §E 자기 검증 (Self-Verification)

각 마일스톤 완료 보고에 포함:

1. AC PASS/FAIL 매트릭스 (acceptance.md §D 기준, 검증 명령 + 실제 출력 verbatim)
2. `cargo test` 전체 결과 (기존 ~92 + 신규, 실패 0)
3. `cargo clippy` NEW 경고 0 (baseline 대비)
4. autotest 리포트 파일 판독 결과 (기존 32 + 신규 flood/switch 체크)
5. bench soak 시나리오 출력 (reader park 발생·해제 확인)
6. @MX 태그 부착 목록 (`grep -n "@MX:" src-tauri/src/session.rs src-tauri/src/output.rs src/terms.ts`)

---

## §F 마일스톤 (독립 검증 가능 단위)

### M1 — 백엔드: flow 상태 + ack 커맨드 + emitter/reader 게이트 (+단위 테스트)

- RED: 워터마크 게이트 판정(순수 함수), park/unpark 조건, 정지 밸브, ack saturating 회계, teardown-under-park join(**ring 임계 초과 유지 상태에서** disarm → signal → join, AC-5), idle 게이트 BUSY, **flow 회계 리셋 3지점**(전환 전이·replay 재무장·밸브 발화 — "outstanding > HIGH에서 전환 → remount 후 live 방출 재개" 포함, AC-14), **replay–pump 경합**(seq 되감김·중복 재방출 없음, AC-15) — 실패 테스트 먼저.
- `session.rs`: 세션별 `FlowState`(emitted/acked AtomicU64 + Mutex/Condvar + disarm 플래그), reader 루프 park 게이트(R4), disarm(R5/R8), 정지 밸브(R6), 회계 리셋 3지점(R15), replay–emitter 상호 배제(R16, §A.9), `require_idle` BUSY 판정(R7).
- `output.rs`: `pump_once` 워터마크 게이트(R3, 히스테리시스), emit 시 `emitted_bytes` 전진(R1), R16 락 범위 준수.
- `commands.rs` + `lib.rs`: `ack_output` 커맨드 등록(R2), `flow_stats(pane_id)` 디버그 커맨드 등록(R1 관측성 — AC-9의 autotest 판정 창구).
- 검증: `cargo test` 신규+기존 전부 green. 프론트 미변경 상태에서 정지 밸브 덕에 기존 동작 보존(ack 전무 → 10s 후 폴백)임을 테스트로 확인.

### M2 — 프론트엔드: ack 콜백 + parsedSeq + 드레인

- `terms.ts`: PTY 출력 경로 `term.write(data, cb)` 전환, ack 누적·배치 플러시(R9), IME 보류분 미ack(R12), `snapshotAndDispose` 드레인(R11).
- `main.ts`: `receivedSeq`/`parsedSeq` 분리(R10), 이벤트 핸들러의 `lastSeq` 직접 전진 제거, replay 데이터 미ack(R13).
- `ipc.ts`: `ackOutput` invoke 래퍼.
- 검증: 수동 스모크(일반 사용 시 ack 흐름·전환 정상) + M3 autotest에서 기계 검증.

### M3 — 검증: autotest flood/switch 체크 + bench + 문서 준비

- autotest 신규 체크 (a) **flood**: pwsh 루프로 대량 출력 생성 → ack 진전 확인, `outstanding <= HIGH_WATERMARK + 1배치` 유지, 활성 팬에 "[output overflow]" 배너 없음, 최종 출력 꼬리 정상 렌더 (AC-9).
- autotest 신규 체크 (b) **switch-under-load**: 홍수 중 워크스페이스 이탈→복귀, 스냅샷/replay 경계 내용 공백 없음 (AC-10, 결함 2 회귀 테스트).
- bench soak 시나리오 추가/확장: reader park 발생·해제, 활성 세션 ring 무한 성장 없음, ack 흐름 중 oldest-drop 없음 (AC-11). bench는 ack 진행을 합성하고 워터마크/타임아웃을 주입한다 (B.5).
- flood/switch 체크는 `flow_stats(pane_id)` 폴링으로 `outstanding`/`emitter_paused`를 기계 판정한다 (AC-9).
- 실기기 수동 검증 노트: Claude Code 세션 홍수 + 한국어 IME 타이핑 (autotest는 실제 IME 불가 — 저장소 공지 한계, acceptance.md §D.3 잔여 위험).

### §F.S — sync 단계 산출물

- `docs/ADR-014-pty-flow-control.md` 신규 (한국어): 배경(결함 2건) → 결정(ack-watermark, reader park, 정지 밸브, seq 이원화) → 트레이드오프(vs oldest-drop 단독, vs 문자 기반 워터마크) → 테스트 구조. 최종 워터마크 값 계측 근거 포함.
- `docs/DEVELOPMENT.md`, `docs/ARCHITECTURE.md` 갱신 (출력 경로 서술에 flow control 반영).
- 기타 동반 문서(README, GUIDE-*)는 사용자 가시 변화(§A.7) 해당 여부 확인 후 갱신 또는 무변경 확인.

---

## §G 안티패턴 (금지)

- registry 전역 락을 쥔 채 condvar park (교착).
- 프론트가 수신 즉시 ack (파싱 완료 전 ack — 결함 1을 못 고침).
- replay 데이터 ack (회계 왜곡, B.2).
- 정지 밸브 없이 park 무한 대기 (죽은 프론트가 자식을 영구 웨지).
- 워터마크를 인라인 리터럴로 산포 (상수 규율 위반).
- autotest를 terminal-f 팬 안에서 실행 (B.4).

---

## §H 참조

- spec.md §B (GEARS R1~R16), §C (제약), §E (범위 제외)
- acceptance.md §D (AC 매트릭스)
- `docs/ADR-004-backpressure-ring-buffer.md` — 유지 불변식
- VS Code terminal flow control (업계 표준 패턴 출처; 문자 기반 100KB/5KB → 본 SPEC은 바이트 기반, §A.1)
- `bin/bench.rs` `pump_dsr` — headless pwsh DSR 헬퍼 (B.5)
