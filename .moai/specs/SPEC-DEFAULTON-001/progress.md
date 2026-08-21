# SPEC-DEFAULTON-001 — progress

## §E.1 Plan-phase Audit-Ready Signal

plan_status: audit-ready
plan_complete_at: 2026-08-21
revision_round: 1

artifacts: spec.md (GEARS R1~R14 — Part A 소급 기록 R1~R8 + Part B R9~R14) + plan.md (M1 + §F.S, Tier M) + acceptance.md (AC-1~AC-13, 논리 13건 — Tier M 상한 16 준수) + progress.md

tier: M · cycle_type: tdd (제거 중심 작업의 RED 대체 규정 — plan.md §A.2) · Route: A (Hybrid Trunk main-direct)
depends_on: 없음
clarification: `[NEEDS CLARIFICATION]` 잔여 0건 — Part A·Part B의 모든 범위·설계 결정은 사용자 승인 완료

### plan-phase 조사 경위 (탐색/인터뷰 세션 내 충족 — 팬아웃·자문 SKIP 사유)

- **Part A는 오키스트레이터가 사용자와 본 세션에서 직접 구현했다.** 요구사항 인터뷰, 자동 설치 방식 옵션 비교(A/B/C, plan.md §A.4), 수용 엣지 3건(§A.4), 문서 갱신 범위는 구현 전후 사용자 승인으로 확정되었고 그 결정사항이 본 SPEC의 §A·plan.md §A.4로 이관되었다. 별도 Socratic 라운드·research 팬아웃은 이미 확정된 결정을 재유도할 뿐이므로 생략했다(SPEC-PTY-FLOW-002 §E.1의 인라인 조사 선례와 동일 형태).
- **Part B 범위도 사용자 승인 완료**: 제거 대상 2커맨드, Copy/Links 토글 유지, 백엔드 `install_pwsh_integration` 보존, ipc/백엔드 status 표면은 run-phase 조건부 결정(§A.1), docs 정리 범위.
- **도메인 자문(Phase 2) SKIP**: 로컬 UX 기본값 전환·팔레트 cleanup은 이 프로젝트 고유 영역으로 외부 도메인(백엔드/프론트엔드/DevOps 전문) 자문의 한계 이득이 없다.

### 검증 증거 (plan-phase, 2026-08-21 작업 트리)

- SPEC ID 정규식 사전 점검 실행: `ID="SPEC-DEFAULTON-001"; [[ "$ID" =~ ^SPEC(-[A-Z][A-Z0-9]*)+-[0-9]{3}$ ]] && echo PASS || echo FAIL` → verbatim 출력 `PASS`
- ID 유일성: `.moai/specs/` 기존 3종(SPEC-PTY-FLOW-001/002, SPEC-WORKSPACE-ROOT-001)과 충돌 없음 — 도메인 `DEFAULTON`은 신규(`mcp__moai__spec_progress` 목록으로 확인)
- frontmatter: canonical 12필드 + 선택 필드 `tier: M` 충족. `phase: "v0.1.3 target"`(릴리스 타깃 — 라이프사이클 토큰 아님. git tag 부재로 v0.1.3는 미출시 상태이며 기존 SPEC 2종과 동일 타깃)
- spec.md §E 범위 제외: `### Out of Scope — <topic>` H3 8건 + 각 항 `-` 불릿 (OutOfScopeRule 충족)
- progress.md 스켈레톤: §E.1~§E.4 리터럴 헤딩(§E.5 미발행 — 3-phase 라이프사이클), §E.1만 기입
- **Part A 사실 grep 재확인**(작업 트리에서 직접 관측):
  - `copyOnSelect|openUrlOnClick` in `src/` → 부팅 읽기 `snap.ui?.copyOnSelect !== false`(`main.ts:878`, `:885`), 모듈 초기값 `let copyOnSelect = true`(`terms.ts:53`), 토글 `main.ts:84-85/:92-93`·제목 `:622/:627`
  - `SHELL_INTG_AUTO_VER|pwshIntegrationAuto|autoInstallShellIntegration` → `main.ts:166/:168/:175/:179/:880-881/:896-897`, `types.ts:68/:71`; 트리거 조건 `!bootInfo.autotest && uiPrefs.pwshIntegrationAuto !== SHELL_INTG_AUTO_VER` + `void` 비동기 호출, `refreshTemplateCommands()`(`:891`) 이후 배치 확인(Read 관측)
  - 실패 경로: `autoInstallShellIntegration()`의 catch → `return`(`:172-177`)이 스탬프 기록(`:179`) 앞에 있음을 Read 관측(AC-5 근거)
- **Part B 전제·제거 표면 grep 재확인**:
  - 팔레트 ID 존재: `shell.pwshMultiline`(`main.ts:631`), `shell.pwshCwd`(`:641`) — 제거 전 baseline 2건
  - `installShellIntegration|pwshIntegrationStatus|pwsh_integration_status` → 정의 `main.ts:104`(내부에서 `ipc.pwshIntegrationStatus` 호출 `:114`), 팔레트 호출부 `:634/:644`, 래퍼 `ipc.ts:117-118`, 백엔드 `commands.rs:1251` + 등록 `lib.rs:175`. **호출자는 `main.ts:114` 1곳, `src/autotest.ts` 사용 0건**
  - `"Shell: Enable"` in `docs/` → GUIDE-command-palette.md `:176/:188`, GUIDE-features-easy.md `:196/:207/:263/:264`, DEVELOPMENT.md `:187`(트러블슈팅 행 — 갱신 대상 발견), ADR-011 `:35`·PLAN-UX-polish.md `:114/:174`(역사 기록 — 보존)
- 부수 관측: `copy.onSelect` 플립·제목의 `=== true`/truthiness 잔존 불일치(spec.md §D·acceptance §D.3에 미수리 기록, 스코프 밖)

### Gaps (plan-phase 미검증)

- plan-auditor 검토 미실시 — run 단계 Phase 1 Plan Audit Gate 소관
- Part A의 build/test 결과(`npm run build` green 39 modules, `cargo test` 142+1+5 green)는 **구현 세션 관측 기록으로 등재** — M1 완료 시 전체 트리 대상 표준 스위트로 재확인된다(Part A·B가 같은 트리)
- 실기기 첫 실행 동작(시나리오 1/2/5) 미관측 — autotest가 자동 설치를 스킵하므로 수동 검증 항목(acceptance §D.3)
- 백엔드 `pwsh_integration_status` 제거 판단의 run-phase 재grep 미실시 — plan.md §A.1 절차(§C 사전 점검 항목 2)

### 개정 이력 — plan-audit iteration 1 대응 (2026-08-21, round 2)

plan-auditor iteration 1 판정: **FAIL 0.81** — 점수는 Tier M 임계값 0.80을 통과했으나 blocking 발견 D1이 FAIL을 강제했다. 보고서: `.moai/reports/plan-audit/SPEC-DEFAULTON-001-review-1.md`. Part A의 build/test 주장은 감사에서 독립 재실행으로 검증되었다(UPGRADE — concern 5). 5건 지적 전량 반영:

| 발견 | 조치 |
|---|---|
| **D1 (major, blocking)** — Part A 6파일 커밋 배정 부재 + 파일 목록 누락 | **Part A 선행 커밋 방식 채택** — `feat(SPEC-DEFAULTON-001): Part A 기본 활성 전환`(명시 pathspec 6파일, `draft → in-progress` 전이 동반). plan.md 헤더에 커밋 스포범 분리(`terms.ts`·`types.ts` 포함) + §F M1 step 0 신설 + §D 필수 갱신 + §E E7에 push 후 `git status --short` 잔여 ` M ` 0건 검증 추가 |
| **D2 (minor, blocking)** — R5(성공 안내)에 대응 AC 부재 | **AC-14 신설**(R5 ↔ 시나리오 1) — `grep -c "Open a NEW PowerShell pane" src/main.ts` → 제거 후 정확히 1건(제거 전 baseline 4건 — `:129`/`:147`/`:155` 제거 대상, `:181` 잔존 대상, 감사 및 재grep으로 확인). 논리 AC 13→14건 — 본 §E.1 상단 artifacts 표기(13건)는 round 1 작성 시점 값이며 본 개정으로 14건 |
| **D3 (optional)** — AC-10 "합리적으로 감소한 값" 판정 문구 | 정확 불변식으로 교체: 142+1+5 passed, 0 failed 유지 — `pwsh_integration_status` 커버 테스트 0건(크레이트 전역 확인)이므로 keep/remove 어느 쪽이든 수치 불변 |
| **D4 (optional)** — plan.md:58 리터럴 대괄호 토큰 | "NEEDS CLARIFICATION 마커 잔여 0건"으로 재표기. 단 본 §E.1 `clarification:` 필드의 백틱 토큰은 audit-ready 동결 지시에 따라 원문 유지(의미는 잔여 0건 선언, 열린 마커 아님) |
| **D5 (optional)** — R6 패턴 라벨 "(Event-detected)" | "(Event-driven)"으로 정정(spec.md R6 — 구조는 처음부터 When+shall로 적합, 라벨만 정정) |

## §E.2 Run-phase Evidence

_<pending run-phase>_

## §E.3 Run-phase Audit-Ready Signal

_<pending run-phase>_

## §E.4 Sync-phase Audit-Ready Signal

_<pending sync-phase>_

## §F Phase 4 Mode Selection

Logged by the orchestrator before the first run-phase Agent() spawn (2026-08-21).

Input parameters: tier M · scope ~5 files (src/main.ts 중심, src/ipc.ts 조건부, docs 3건) · domain count 2 (frontend TS palette / docs) · file mix TS + Markdown · concurrency benefit LOW (removal-only, single-file-centered edits) · Agent Teams prereqs N/A (Mode 3 retired)

Mode evaluation: trivial ✗ (multi-file removal + commit choreography) · background ✗ (write-capable) · agent-team ✗ (RETIRED) · parallel ✗ (coding-heavy, cross-file dependency — Anthropic coding-task parallelism caveat) · **sub-agent ✓ SELECTED** · workflow ✗ (~5 files, below the ~30-file mechanical threshold)

Decision: sub-agent

Justification: 단일 마일스톤(M1) 제거 중심 코딩 작업이 `src/main.ts` 한 곳에 집중되고 ipc/docs 정리가 그 결과에 의존하는 구조라 순차 단일 에이전트 봉투(Mode 5)가 적합하다. Implementation Kickoff Approval은 본 선택 적용 전에 사용자 승인 완료(자율 실행, 2026-08-21). Plan Audit Gate Phase 1 재실행은 skip-eligible 3조건(PASS 판정 · 1.00 ≥ Tier M 임계 0.80 · 감사 후 plan-artifact 무변경)이 모두 성립하여 생략하며, 본 사유는 run-phase 위임 프롬프트 Section A에 기록한다.
