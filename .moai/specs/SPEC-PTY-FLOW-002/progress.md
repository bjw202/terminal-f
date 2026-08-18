# SPEC-PTY-FLOW-002 — progress

## §E.1 Plan-phase Audit-Ready Signal

plan_status: audit-ready
plan_complete_at: 2026-08-18
revision_round: 3 (Round 2 PASS 0.92; 잔여 R2-F1~F6 반영 완료)
plan_audit: iteration 1 FAIL 0.71 → iteration 2 **PASS 0.92** (Tier M 임계값 0.80). 추가 감사 라운드 불필요; Round 2 잔여 지적(R2-F1~F6)은 신규 AC 없이 제자리 정정으로 반영

artifacts: spec.md (GEARS R1~R13) + plan.md (M1~M2 + §F.S, Tier M) + acceptance.md (논리 AC 16건 = AC-1~AC-16, AC-10은 sub-ID 6개를 1건으로 계수 — Tier M 상한 16 준수, **증가 없음**) + research.md (증거 E1~E26) + progress.md

### 개정 이력 — plan-audit iteration 2 잔여 지적 반영 (2026-08-18, round 3)

Round 2 판정 **PASS 0.92**. 아래는 통과 후 남은 정밀도 지적으로, 전부 **제자리 정정**이며 AC 개수는 16건 그대로다. 6건 모두 작업 트리에서 주장을 재확인한 뒤 반영했다.

| 발견 | 확인된 사실 | 조치 |
|---|---|---|
| **R2-F1** — AC-16(a) grep이 주석을 함께 세어 올바른 코드에서 실패 | `grep -c "writeParsedNoAck" src/main.ts` = **6** (호출 5 + `main.ts:316` 주석 1) / `grep -c "terms.writeParsedNoAck(" src/main.ts` = **5** | AC-16 (a) 패턴을 `terms.writeParsedNoAck(`로 한정하고 기대값 5 유지. 신설 `#### AC-16 판정 근거` 절에 호출부 5건 전량 표로 열거(`:300` 스냅샷 / `:309` 오버플로 배너 / `:317` replay / `:324` exit 배너 / `:328` no-session 배너) |
| **R2-F2** — B10의 "bench가 `with_config` 주입" 완화책이 실행 불가 | registry 세션은 `FlowState::default()` 하드코딩(`session.rs:522`, `session.rs:1217`), `Arc<PtySession>` 배후라 사후 수정 불가. bench가 이 불가능성을 `bench.rs:263-271` 주석으로 이미 기록 | B10을 **env 오버라이드**로 교체 — `FlowConfig::default()`가 `TERMF_FLOW_STALL_TIMEOUT_MS`를 1회 파싱(미설정·실패 시 기본 10s 불변), 구현 지점은 `flow_state.rs`. `with_config` 기각 사유와 "신규 임계값 상수 아님" 근거 명기. 변경 파일 목록의 허위 서술("`bench.rs` 1줄, 스코프 확장 아님") 제거하고 실제 주입 지점을 `flow_state.rs`로 정정. AC-13 검증 명령에 env 변수 반영. research.md **E23** 신설(반증 인용 `bench.rs:263-271`) |
| **R2-F3** — bench 표본 JSON은 손수 조립이라 `FlowStats` 필드 추가가 자동 반영되지 않음 | `bench.rs:318-327`(Phase A) / `bench.rs:358-366`(Phase B) 모두 6키 명시 나열 | M2 GREEN 산출물에 두 블록의 `emitter_valve_fired` 추가를 명시하고, 누락 시 AC-13 판정이 필드 부재로 성립하지 않음을 경고로 병기. research.md **E24** 신설 |
| **R2-F4** — `autotest.ts`의 2인자 `writeOutput` 호출이 AC-16 판정에 걸릴 우려 | `autotest.ts:607`, `autotest.ts:623` — IME 버퍼링 검사용 합성 청크 주입(`seq`·`byteLen` 부재가 정상) | AC-16 판정 근거 절에 (b)항 추가 — grep 범위가 `src/main.ts` 한정이므로 두 건은 판정 대상 아니며 `meta?` 전환 후에도 2인자 호출이 유효함을 명기 |
| **R2-F5** — plan.md §H의 AC 범위 표기가 구식 | `plan.md:255` `AC-1~AC-15` | `AC-1~AC-16 (논리 16건, AC-10은 sub-ID a~f를 1건으로 계수)`로 정정 |
| **R2-F6** — AC-4의 "정확히 1건" 기준이 주석 때문에 성립하지 않음 | 현행 `data.length` 매치 4건: `:199` 주석 / `:200` ack 산출 / `:210` `outBufLen` / `:212` 보류 누적 | AC-4 기준문에 `:199` 주석 문구 수정이 기준 성립 조건임을 명시하고, **M1 산출물에 주석 정리를 필수 항목으로 추가**(선택적 정리가 아님을 명기). research.md **E25/E26** 신설 |

### 개정 이력 — plan-audit iteration 1 대응 (2026-08-18)

plan-auditor iteration 1 판정: **FAIL 0.71** (조화평균, Tier M 임계값 0.80) — Testability 0.50 / Traceability 0.75. 보고서: `.moai/specs/SPEC-PTY-FLOW-002/plan-audit.md`. Must-Pass 7항목은 전항 통과였고 감점 원인은 BLOCKER 1건 + MAJOR 4건 + MINOR 3건이었다. 사용자 승인 설계(D1/D2/D3)는 재개방하지 않았으며 아티팩트 수준 정정만 수행했다. 감사에서 "확인된 강점"으로 명시된 항목(밸브 평가 지점 타당성, R9 레이스 분석, M1/M2 분리 근거, research.md 증거표 E1~E16)은 재작업하지 않았다.

| 발견 | 조치 |
|---|---|
| **F1 (BLOCKER)** — `report.ok` 체인이 흐름 제어 체크를 구조적으로 배제(`autotest.ts:786-815` vs `flowOk` 824-831)하여 종단 가드가 아무것도 측정하지 못함 | 판정 기준을 `report.flowOk`로 정정하고 `flowOk` 식별자를 아티팩트 전반에 명시. 신규 6체크를 `flowOk` 집계식에 **append-only 확장**으로 편입하기로 확정, 그 확장이 PRESERVE/§G 금지의 명시적 예외임을 명문화 |
| **F2 (MAJOR)** — `FlowStats`에 `valveFired` 필드가 존재하지 않는데 AC-8이 "기존 필드 불변"을 전제 | `valveFired` + `emitterValveFired` **두 필드 신규 노출**로 결정을 명시. AC-8을 신규 노출 + 기존 5필드 불변 판정으로 재작성 |
| **F3 (MAJOR)** — R6(미ack 유지)에 대응 AC 부재 | **AC-16 신설** — `writeParsedNoAck` 호출부 5건 + 합성 배너 `meta` 미전달 + `writeParsedNoAck` 정의 3축 grep 판정 |
| **F4 (MAJOR)** — `byteLen?` 선택 파라미터가 B1의 "컴파일 시점 포착" 주장과 상충(미전달 시 조용한 0-ack 재발) | `writeOutput(paneId, data, meta?: { seq; byteLen })` **객체 묶음**으로 확정. 선택 파라미터 기각 사유를 §A.2에 기록, B1 완화책 재작성 |
| **F5 (MAJOR)** — 결함은 TS에 있는데 재현 테스트는 Rust에만 존재 / 밸브가 종단 판정을 무력화 | R12 적용 범위를 백엔드로 한정 명시 + TS 배선 갭을 §D.3 **최우선 잔여 위험**으로 등재. **AC-10e**(`acked/emitted >= 0.9`) + **AC-10f**(`emitterValveFired` 0 유지) 신설로 밸브 마스킹 차단 |
| **F6 (MINOR)** — bench Phase A 10초 무ack 구간과 기본 `stall_timeout` 10초 경계 충돌 | §B **B10** 신설 + 완화책 확정(bench가 확대 `stall_timeout` 60s 주입). AC-13 기준문에 반영, 변경 파일에 `bench.rs` 추가 |
| **F7 (MINOR)** — §A.3 판정 규칙에 `paused_since` 무장 단계 부재 → 밸브 영구 미발화 | **규칙 0(무장)** 신설. 전이 경로와 전이 없는 직접 호출 경로(`flow_tests.rs:22-45`) 양쪽을 덮도록 명시 |
| **F8 (MINOR)** — M2 GREEN 산출물에 `types.ts` FlowStats 필드 추가 누락 | M2 GREEN 목록에 `src/types.ts` FlowStats 두 필드 추가를 명시(누락 시 §A.5 표본 수집이 tsc red) |
| F9 (optional) | AC-13을 **기능 무회귀**로 축소, 성능은 §D.3 잔여 위험으로 이관(미계측 명시) |
| F10 (optional) | **AC-ID 네임스페이스 규약** 신설 — 선행 SPEC 참조는 `FLOW-001 AC-N` 접두 표기로 전량 정규화 |
| F11 (optional) | AC-4를 완전 이진화 — `data.length` 매치가 **정확히 1건**(`outBufLen`)만 남을 것 + `ackBytes`/`heldAckBytes` 대입식 우변 검증 |
| F12 (optional) | AC 계수 표기 통일 — 논리 AC 16건(AC-10 sub-ID 6개는 1건 계수), Tier M 상한 16 준수 |
| F13 (optional) | R8 패턴 라벨 `Event-detected` → `Event-driven` 정정 |
| F14 (optional) | 알려진 잔여 누수 2경로(`terms.ts:193` 조기 반환 / `terms.ts:313` ack IPC 실패 삼킴)를 research.md **E17/E18** + spec.md §D 엣지 표 + plan.md **B11/B12** + §D.3에 등재 |
| F15 (optional) | AC 매트릭스에 **시나리오 열** 추가 — 각 AC ↔ §B GWT 시나리오 대응 명시 |

추가 증거: research.md에 **E19~E22** 등재(FlowStats 5필드 현황 / `report.ok` 배제 구조 / TS 러너 부재 / bench Phase A 경계) — 위 조치의 근거를 작업 트리 file:line으로 고정.
tier: M · cycle_type: tdd (재현-우선) · Route: A (Hybrid Trunk main-direct)
depends_on: SPEC-PTY-FLOW-001 (`status: completed` — depends_on 사전 점검 충족)
clarification: `[NEEDS CLARIFICATION]` 잔여 0건 (plan.md §A.7 — 모든 설계 결정 사용자 승인 완료)

### plan-phase 조사 경위 (Phase 1 / Phase 2 SKIP 사유)

- **Phase 1 (research 팬아웃) SKIP**: 오키스트레이터가 본 세션에서 코드베이스 조사를 **인라인으로 수행**하여 근본 원인을 file:line 수준까지 확정했다. 별도 read-only 리서치 팬아웃은 이미 확보된 증거를 재수집할 뿐이므로 생략했다. 조사 결과는 research.md §B 증거표(E1~E16)로 이관되었으며, 각 증거는 작업 트리에서 재확인 가능하다.
- **Phase 2 (도메인 전문가 자문) SKIP**: 결함 도메인이 이 프로젝트 고유의 PTY 흐름 제어 회계이고, 선행 SPEC-PTY-FLOW-001의 설계 맥락이 이미 문서화되어 있어 외부 도메인 자문의 한계 이득이 없다. 백엔드/프론트엔드 접점은 단일 필드 반사 계약으로 국한된다.
- **설계 결정 확정 경로**: D1(이벤트 `byteLen` 반사 ack) / D2(emitter 정지 안전밸브) / D3(재현-우선 테스트 의무) 3건 모두 사용자 승인 완료. 대안 기각 사유는 plan.md §A.1 / §A.3 및 research.md §E에 기록.

### 검증 증거 (plan-phase)

- SPEC ID 정규식 사전 점검 실행: `[[ "SPEC-PTY-FLOW-002" =~ ^SPEC(-[A-Z][A-Z0-9]*)+-[0-9]{3}$ ]]` → verbatim 출력 `PASS`
- frontmatter: canonical 12필드 + 선택 필드(`tier`, `depends_on`) 충족. `phase: "v0.1.3 target"`(릴리스 타깃 — 라이프사이클 토큰 아님)
- spec.md §E 범위 제외: `### Out of Scope — <topic>` H3 7건 + 각 항목 `-` 불릿 (OutOfScopeRule 충족)
- 증거표 file:line 전량 작업 트리에서 직접 확인 (`output.rs:31-38/90-92`, `flow_state.rs:130/171/203/275`, `terms.ts:200/212/244/254`, `types.ts:229-235`, `session.rs:684/803/936-945`, `main.ts:290-330`, `autotest.ts:273`)

### Gaps (plan-phase 미검증)

- 실기기 `flow_stats` 원시 표본 미수집 — 근본 원인은 코드 경로 추적으로 확정, 실기기 수치는 M2 autotest(AC-10)가 담당
- PowerShell 콘솔 출력 인코딩(UTF-8 여부) 미확인 — M2 착수 시 선행 확인 필요 (plan.md B6)
- plan-auditor 검토 미실시 — run 단계 Phase 1 Plan Audit Gate 소관

## §E.2 Run-phase Evidence

### M1 — 단위 통일 + 재현-우선 회귀 테스트 (2026-08-18)

작업 트리: worktree `agent-a37d28d279bb94f48` (base `288b4a2`, Route A). 아래 모든 출력은 이 트리·이 실행에서 관측한 verbatim이다.

#### RED (AC-2) — 수정 이전 실패 출력 verbatim

`cd src-tauri && cargo test flow002` (구현 전, 신규 3테스트 추가 직후):

```text
error[E0609]: no field `byte_len` on type `&PtyOutputEvent`
   --> src\flow_tests.rs:618:19
    |
618 |     assert_eq!(ev.byte_len, ev.data.len());
    |                   ^^^^^^^^ unknown field
    |
    = note: available fields are: `workspace_id`, `pane_id`, `session_id`, `seq`, `data`

error[E0609]: no field `byte_len` on type `&PtyOutputEvent`
   --> src\flow_tests.rs:621:12
    |
621 |         ev.byte_len as u64,
    |            ^^^^^^^^ unknown field
    |
    = note: available fields are: `workspace_id`, `pane_id`, `session_id`, `seq`, `data`

For more information about this error, try `rustc --explain E0609`.
error: could not compile `terminal-f` (lib test) due to 4 previous errors
```

계획된 RED 의 첫 형태다(plan §F M1 — `byte_len` 필드 부재 컴파일 실패). "필드는 있으나 의미론 미통일" 런타임 실패 형태는 §A.1 구현(단일 산출)이 한 번에 적용되어 출현하지 않았다 — 첫 컴파일 성공 시점에 짝 테스트가 곧바로 green.

#### AC 매트릭스 (M1) — 전 항목 이 트리·이 실행 관측

| AC | 판정 | 검증 명령 | 실출력 (verbatim 발췌) |
|---|---|---|---|
| AC-1 | PASS | `cargo test --lib flow002` + `grep -n "byte_len" src-tauri/src/output.rs` + `grep -n "byteLen" src/types.ts` | `flow002_ac1_banner_included_bytelen_same_source_as_emit ... ok` (배너 포함: record_emit 가산값 == ev.byte_len == ev.data.len()). grep: output.rs `41: pub byte_len: usize`, `98: let byte_len = data.len();`, `99: record_emit(byte_len)` / types.ts `236: byteLen: number;` |
| AC-2 | PASS | RED 출력 상기(구현 전 캡처 — 사후 재현 불가) | `error[E0609]: no field byte_len` ×4 → `could not compile terminal-f (lib test) due to 4 previous errors` |
| AC-3 | PASS | `cargo test --lib flow002` | `flow002_ac3_bytelen_ack_drains_outstanding_and_resumes ... ok` — 이모지(4B/2u) 포함 페이로드, ev.byteLen ack → `outstanding == 0` + 게이트 재개 true |
| AC-4 | PASS | `grep -n "data.length" src/terms.ts` + `grep -n "ackBytes\|heldAckBytes" src/terms.ts` | data.length 매치 정확히 1건: `219: view.outBufLen += data.length;` (용량 캡 계산 — 정당 용법). ack 누적식: `222: view.heldAckBytes += meta.byteLen;` — 우변 전부 byteLen 유래 |
| AC-5 | PASS | `grep -n "heldAckBytes" src/terms.ts` + `grep -c "TextEncoder" src/terms.ts` | 가산 대상 개별 이벤트 `meta.byteLen` 합(문자열 재산정 없음), TextEncoder 매치 0 |
| AC-9 | PASS | `cd src-tauri && cargo test` | `test result: ok. 138 passed; 0 failed;` (lib — 기존 135 + 신규 3). 전 스위트 green(138+1+5), 신규 실패 0 |
| AC-12 | PASS | `npx tsc --noEmit` + `cargo clippy --all-targets` | tsc `exit=0`. clippy 경고 위치: bench.rs:117/:144, state.rs:419, paste.rs:75, spool.rs:73 — 전부 사전 존재(무관 파일). M1 변경 파일(output.rs·flow_tests.rs) 경고 0건 |
| AC-16 | PASS | (a) `grep -c "terms.writeParsedNoAck(" src/main.ts` (b) `grep -n "terms.writeOutput(" src/main.ts` (c) `grep -n -A 8 "export function writeParsedNoAck" src/terms.ts` | (a) `5` · (b) 4건 — `337`/`815` 실 PTY `{ seq: ev.seq, byteLen: ev.byteLen }` 전달, `348`/`827` 배너 2인자 · (c) `294: return writeParsed(view, data, seq, 0);` 유지 |

#### 보조 검증 (M1)

- `cargo fmt --check`: 신규 영역(flow_tests.rs :480 이후) 매치 0건. 저장소 전체 fmt 드리프트는 사전 존재(무관 파일 15+) — M1 스코프 외, 미손대기.
- `git diff --stat`: `output.rs +12/-2`, `flow_tests.rs +186`, `terms.ts +28/-13`, `main.ts +5/-2`, `types.ts +2`, `Cargo.lock`(이미 커밋된 Cargo.toml 0.1.2 로의 버전 동기 — 의존성 변화 없음). §D PRESERVE 대상(flow_state.rs 상수·밸브, session.rs reset 3지점, autotest.ts 32체크 체인) 무변경.

#### Gaps / Residual-risk (M1)

- TS 배선 테스트 러너 부재(acceptance §D.3 구조적 갭) — terms.ts 변경은 tsc + 구조 grep(AC-4/5/16)로만 검증됨. 종단 수치 판정(AC-10e)은 M2 autotest 소관.
- bench 실행(AC-13)·autotest 실행(E7)·@MX WARN/ANCHOR 부착(AC-14)은 M2 검증 대상 — M1 범위 아님.
- Blocker 0건 — 본문 수정 요청 사유 없음.

## §E.3 Run-phase Audit-Ready Signal

_<pending run-phase>_

## §E.4 Sync-phase Audit-Ready Signal

_<pending sync-phase>_

## §F Phase 4 Mode Selection

Decision: sub-agent (Mode 5 — 마일스톤별 순차 `manager-develop` 위임)
기록: 2026-08-18, run 진입·착수 승인 게이트 직전. 감사 캐시 lookup MISS(현 해시 `826c5a1b…` — R2-F1~F6 정정 반영 후 저장 판정 없음; plan-auditor iteration 2 판정문에 "추가 감사 라운드 불필요" 명시됨).

입력 파라미터:
- tier: M
- scope: 추정 변경 파일 7건 (`src-tauri/src/output.rs`, `flow_state.rs`, `flow_tests.rs`, `bin/bench.rs`, `src/terms.ts`, `types.ts`, `autotest.ts`)
- domain count: 2 (Rust 백엔드 회계·밸브 / TS 프론트엔드 배선) — Mode 4 임계(≥3) 미달
- file language mix: Rust + TypeScript
- concurrency benefit: LOW (코딩 중심 구현)
- Agent Teams prereqs: N/A (Mode 3 폐기)

| 모드 | 선택 | 근거 |
|---|---|---|
| trivial | 아니오 | 다중 파일·시맨틱 변경 |
| background | 아니오 | 쓰기 작업 — 결과를 기다려야 하는 구현 |
| agent-team | 아니오 | RETIRED (Mode 3 폐기) |
| parallel | 아니오 | 도메인 2개(임계 미달) + 코딩 중심 작업 |
| sub-agent | **선택** | Tier M·마일스톤 2개의 표준 순차 경로 |
| workflow | 아니오 | ~30파일 기계적 균일 변환 아님 (7파일 시맨틱 구현) |

근거: 코딩 중심 작업으로 Anthropic 코딩-병렬성 주의(연구에 비해 진짜 병렬화 가능한 태스크가 적음)에 따라 순차 위임이 안전한 기본값이다. manager-kanban 진입 임계(마일스톤 ≥3 AND 파일 ≥10) 미달이며, plan.md §F의 마일스톤 분리 근거(M2 밸브가 M1 단위 통일의 결함 판정을 가리는 것 방지, B4)도 M1→M2 순차 실행을 지지한다.
