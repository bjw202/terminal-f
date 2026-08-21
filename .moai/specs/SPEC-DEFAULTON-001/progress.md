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

**실행 환경 특기(2026-08-21)**: run-phase는 오케스트레이터 worktree 격리 정책에 따라 격리 워크트리(분기 `worktree-agent-aefdeb8f2f69fb788`)에서 수행됐다. Part A 구현 6파일은 주 체크아웃의 미커밋 상태였으므로 바이트 복사로 워크트리에 이관했다(이관 전 `diff -u` 대조로 Part A 설계 구조물 — 부팅 읽기 `!== false`·스탬프 상수·자동 설치 본체·트리거 — 와 내용 일치 확인). `core.autocrlf=true` 정규화로 커밋 blob은 기존 인덱스와 동일한 LF다. 최종 반영은 `git push origin HEAD:main`(Route A main-direct).

### A.1 keep/remove 판정 — status IPC 표면 3곳 제거

plan.md §A.1의 run-phase 재grep 의무 이행(제거 전, HEAD 8f75808 + Part A 트리):

- `grep -n "pwshIntegrationStatus" src/ipc.ts src/main.ts` → `src/ipc.ts:117`(래퍼 정의) + `src/main.ts:114`(유일 호출자 — 죽은 흐름 내부)
- `grep -n "pwshIntegrationStatus\|installShellIntegration\|pwsh_integration_status" src/autotest.ts` → **0건(exit 1)** — autotest 무관계 재확인
- `grep -rn "pwsh_integration_status" src-tauri/src` → `commands.rs:1251` + 등록 `lib.rs:175`만 — 다른 호출 경로 없음
- `grep -rn "pwsh_integration_status" src-tauri/ --include="*.rs"`(위 2건 제외) → 0건 — **커버 테스트 0건 재확인**(AC-10 불변 근거)

**판정: 제거.** (i) `src/ipc.ts` 래퍼 2줄 + 주석 재서술, (ii) `commands.rs` `pwsh_integration_status` 커맨드(doc 주석 포함 29줄), (iii) `lib.rs` 등록 1줄. 유지 사유 없음(상태 표시 기능 복귀 계획 부재). 공유 자산 보존: `PwshIntegrationInfo`(install 반환형)·`ShellIntegrationFeature`(install 래퍼 파라미터)·`cached_profile_path`/`PROFILE_PATH_CACHE`(install 경로 :1286 계속 사용 — 죽은 흐름 설명 doc 주석만 자동 설치 근거로 재서술)·`shellint.rs::is_installed`(shellint.rs 내부 :70 + 7개 단위테스트가 계속 사용).

### RED 상당 증거(제거 전 존재 grep verbatim — plan.md §A.2)

```
$ grep -n "shell.pwshMultiline\|shell.pwshCwd" src/main.ts
631:    id: "shell.pwshMultiline",
641:    id: "shell.pwshCwd",
$ grep -n "installShellIntegration" src/main.ts
104:async function installShellIntegration(
634:      installShellIntegration("multiline", {
644:      installShellIntegration("cwd", {
$ grep -c "Open a NEW PowerShell pane" src/main.ts
4
$ grep -n "Shell: Enable" docs/GUIDE-command-palette.md docs/GUIDE-features-easy.md docs/DEVELOPMENT.md
docs/GUIDE-command-palette.md:176 / :188 (커맨드 문서 2행)
docs/GUIDE-features-easy.md:196 / :207 / :263 / :264 (수동 설치 산문 2곳 + 메뉴 사전 2행)
docs/DEVELOPMENT.md:187 (트러블슈팅 행)
```

### AC-1~AC-14 판정 (GREEN — HEAD 769a0fe + M1 변경 트리에서 관측)

| AC | 판정 | 근거(명령 → 관측) |
|---|---|---|
| AC-1/AC-2 | PASS | `grep -n "copyOnSelect !== false" src/main.ts` → `:795`, `:802`(2건) · `grep -n "let copyOnSelect = true" src/terms.ts` → `:53`(1건) |
| AC-3/AC-4 | PASS | `grep -n "SHELL_INTG_AUTO_VER\|pwshIntegrationAuto\|autoInstallShellIntegration" src/main.ts` → 상수 `:103`, 정의 `:105`, 스탬프 `:116`, 스냅샷 복원 `:797-798`, 트리거 `if (!bootInfo.autotest && …)` `:816` + `void …()` `:817` · `grep -n "pwshIntegrationAuto" src/types.ts` → `:71` |
| AC-5 | PASS | `grep -n -B6 "pwshIntegrationAuto = SHELL_INTG_AUTO_VER" src/main.ts` → catch→`return`(`:113`)가 루프 내 실패 분기, 대입(`:116`)은 루프 완전 통과 후 도달 |
| AC-6 | PASS | `grep -c "shell.pwshMultiline\|shell.pwshCwd" src/main.ts` → **0**(baseline 2건) |
| AC-7 | PASS | `grep -c "copy.onSelect" src/main.ts` → 1(`:558`) · `grep -c "links.toggleOpen" src/main.ts` → 1(`:563`) |
| AC-8 | PASS | `installShellIntegration` in main.ts → 0(exit 1) · `pwshIntegrationStatus` in ipc.ts+main.ts → 0(exit 1) · `pwsh_integration_status` in src/+src-tauri/src → 0(exit 1). 제거 완료라 유지 사유 기록 불필요 |
| AC-9 | PASS | `grep -n "install_pwsh_integration" src-tauri/src/commands.rs` → `:1255` · `ls src-tauri/src/shellint.rs` → 존재 · `grep -n "installPwshIntegration" src/main.ts` → `:108`(자동 설치 호출) |
| AC-10 | PASS | `cd src-tauri && cargo test` → **142 + 1 + 5 passed, 0 failed**(baseline과 수치 동일 — 제거 커맨드 커버 테스트 0건 예측 적중, 제거된 테스트명 없음) |
| AC-11 | PASS | `npm run build` → tsc --noEmit exit 0 + `✓ 39 modules transformed`(번들 495.96→492.08 kB, `✓ built in 1.32s`) |
| AC-12 | PASS | 팬 밖 `TERMF_AUTOTEST=1 TERMF_REPORT_PATH=… npx tauri dev` → `src-tauri/autotest-report.json`(2026-08-21 13:09)에서 **`"ok": true`** · `"errors": []` · checks 48항목 전부 true · `flowOk: true` · switch p95 62.8ms · soak rss ×1.022 |
| AC-13 | PASS | `grep -rn "Shell: Enable" docs/GUIDE-command-palette.md docs/GUIDE-features-easy.md` → **0**(exit 1) · `grep -n "Shell: Enable" docs/DEVELOPMENT.md` → 0(exit 1, `:187` 행 제거 + `:191` 재서술) · ADR-011 `:35`·PLAN-UX-polish `:114`/`:174` 보존(역사 기록, 무변경 diff로 확인) |
| AC-14 | PASS | `grep -c "Open a NEW PowerShell pane" src/main.ts` → **1**(`:118` 자동 설치 안내 — baseline 4건 중 죽은 흐름 3건 소멸 확인) |

### 스위트 원본 출력(attribution: this run, this tree)

- **baseline**(HEAD 8f75808 + Part A 트리, 커밋 전): `npm run build` → `✓ built in 1.31s`(39 modules, exit 0) · `cargo test` → unittests 142 passed / pipe_smoke 1 passed / pty_smoke 5 passed, 0 failed
- **M1 완료**(HEAD 769a0fe + M1 변경): `npm run build` → `✓ built in 1.32s`(tsc exit 0) · `cargo test` → 동일 142+1+5, 0 failed
- **autotest**(HEAD 769a0fe + M1 변경): 리포트 `src-tauri/autotest-report.json` — startedAt `2026-08-21T04:07:36.228Z`, `"ok": true`
- 스코프 검증: `git diff --stat` → Part B 7파일(+23/−178) = plan.md 예측 파일 목록과 정확히 일치 · PRESERVE 대상(terms.ts/types.ts/autotest.ts/ADR-011/PLAN-UX-polish) diff 0

### 커밋 기록

| # | SHA | 서브젝트 | 내용 |
|---|---|---|---|
| 1 | `769a0fe` | feat(SPEC-DEFAULTON-001): Part A 기본 활성 전환 | Part A 6파일 + spec.md `draft → in-progress` 전이(7 files, +81/−36) |
| 2 | M1 커밋(§E.3에 백필) | feat(SPEC-DEFAULTON-001): M1 팔레트 설치 커맨드 제거·죽은 흐름 정리 | Part B 7파일 + progress.md §E.2/§E.3 |
| 3 | 백필 커밋 | chore(SPEC-DEFAULTON-001): M1 §E.3 run_commit_sha 백필 | 플레이스홀더 → 실제 SHA(schema D3 예외 패턴) |

### run-phase 발견·처치(plan §A.3 표를 넘어선 항목 — 사유 기록)

1. **GUIDE-command-palette.md §5 통짜 제거 + 재번호(§6~§9 → §5~§8)**: 커맨드 2행만 지우면 섹션 제목·"직접 실행할 때는 … 동의를 받은 뒤" 산문이 유령으로 남는다. §5 전체(37줄) 제거 후 후속 섹션 재번호. 내부 교차참조 2건 동행 수정(§1의 "§5의 live directory tracking 설정" → 셸 통합 설명+기능 가이드 링크, 안전 원칙의 "§7·§8" → "§6·§7"). 문서 간 섹션 앵커 참조 0건(제거 전 grep)이라 파급 없음.
2. **DEVELOPMENT.md:191 함정 항목 (2) 재서술**: "팔레트 명령을 실제로 실행해야 설치됨"은 plan §A.3 grep("Shell: Enable")에 걸리지 않았으나 R13·시나리오 6(안내 문서에 죽은 메뉴 안내 부재) 위반 잔여다. "첫 실행 자동 설치로 설치됨(스탬프 기록 머신 재설치는 `SHELL_INTG_AUTO_VER` 상향)"으로 교체 — B10의 run-phase 확장 적용.
3. **commands.rs `PROFILE_PATH_CACHE` doc 주석 재서술**: 원 주석의 "status query … confirm dialog … per click"은 죽은 흐름 설명(acceptance §D.1 Readable 위반) → 첫 실행 자동 설치의 feature별 2회 연속 해석 근거로 재서술.
4. **ipc.ts 주석 재서술**: "status + install … the UI confirms first"(Opt-in 서술) → install 단독·첫 실행 자동 설치 유일 호출자 서술.
5. **main.ts modal import 축소**: tsconfig `noUnusedLocals: true`라 `confirmModal`·`listModal` 미사용 임포트가 tsc를 깬다. `import { promptModal } from "./modal"`로 축소(잔여 호출자 5곳). 두 함수 자체는 modal.ts 모듈 API로 보존.
6. **@MX:ANCHOR 부착(M1 step 6 선택 항목 적용)**: 자동 설치 트리거에 한국어 ANCHOR+REASON+SPEC 3행. 태그 문구에 AC grep 식별자를 넣지 않아 증거 grep 오염 없음.
7. **`src-tauri/Cargo.toml` LF→CRLF 팬텀**: autotest cargo 빌드 후 ` M ` 표시되나 `git diff` 내용 0줄(autocrlf 팬텀). 커밋에서 제외 처리.

### 미검증(Gaps) — 수동 검증 항목 이월

- **실기기 첫 실행 자동 설치·안내(시나리오 1)·저장 `false` 존중(시나리오 2)·업그레이드 1회 재설치(시나리오 5)**: R2가 autotest에서 자동 설치를 스킵하므로 autotest로 검증 불가(acceptance §D.3 잔여 위험 그대로). 격리 워크트리 실행 환경에서 실사용 프로필 실험은 부적절해 미실시 — 수동 검증 항목으로 이월.
- 크로스 플랫폼(linux/macOS) 빌드 미실시 — SPEC 표면이 Windows pwsh 전용, CI 부재.

### 잔여 위험

- acceptance §D.3 기재 그대로: TS 러너 부재 구조 갭(구조 grep + autotest 두 축), Copy 토글 제목 표시 잔여 불일치(후속 후보), (a) 1회 재설치 사용자 혼란(실사용 피드백 대기), pwsh 콜드 스타트 안내 지연.

## §E.3 Run-phase Audit-Ready Signal

```yaml
run_status: audit-ready
run_complete_at: 2026-08-21
run_commit_sha: 633e13b   # M1 커밋 — 플레이스홀더에서 백필(schema D3 예외 패턴)
ac_pass_count: 14
ac_fail_count: 0
preserve_list_post_run_count: 11   # §D PRESERVE 트립와이어 11종(§E.2 AC-1~5/7/9 근거 열) 전부 관측
l44_pre_commit_fetch: "git fetch origin main → rev-list --left-right origin/main...HEAD → 0 0 (2026-08-21, 커밋 전)"
l44_post_push_fetch: "push 후 run-phase 완료 보고(E7)에서 verbatim 인증 — 본 커밋은 푸시에 선행"
new_warnings_or_lints_introduced: 0   # tsc --noEmit exit 0 · cargo test 0 failed · 신규 경고 0
cross_platform_build:
  windows: "pass — npm run build(39 modules) + cargo test 142+1+5 + autotest ok:true (격리 워크트리)"
  linux: "not_run — SPEC 표면이 pwsh(Windows) 전용"
  macos: "not_run — 동일 사유"
total_run_phase_files: 11   # Part A 6 + spec.md(frontmatter) + Part B 신규 3(ipc.ts/lib.rs/DEVELOPMENT.md) + progress.md(§E.2/§E.3)
m1_to_mN_commit_strategy: "단일 M1 — Part A 선행 커밋(769a0fe) → M1 커밋 → §E.3 SHA 백필 chore 커밋 → push origin HEAD:main"
```

## §E.4 Sync-phase Audit-Ready Signal

_<pending sync-phase>_

## §F Phase 4 Mode Selection

Logged by the orchestrator before the first run-phase Agent() spawn (2026-08-21).

Input parameters: tier M · scope ~5 files (src/main.ts 중심, src/ipc.ts 조건부, docs 3건) · domain count 2 (frontend TS palette / docs) · file mix TS + Markdown · concurrency benefit LOW (removal-only, single-file-centered edits) · Agent Teams prereqs N/A (Mode 3 retired)

Mode evaluation: trivial ✗ (multi-file removal + commit choreography) · background ✗ (write-capable) · agent-team ✗ (RETIRED) · parallel ✗ (coding-heavy, cross-file dependency — Anthropic coding-task parallelism caveat) · **sub-agent ✓ SELECTED** · workflow ✗ (~5 files, below the ~30-file mechanical threshold)

Decision: sub-agent

Justification: 단일 마일스톤(M1) 제거 중심 코딩 작업이 `src/main.ts` 한 곳에 집중되고 ipc/docs 정리가 그 결과에 의존하는 구조라 순차 단일 에이전트 봉투(Mode 5)가 적합하다. Implementation Kickoff Approval은 본 선택 적용 전에 사용자 승인 완료(자율 실행, 2026-08-21). Plan Audit Gate Phase 1 재실행은 skip-eligible 3조건(PASS 판정 · 1.00 ≥ Tier M 임계 0.80 · 감사 후 plan-artifact 무변경)이 모두 성립하여 생략하며, 본 사유는 run-phase 위임 프롬프트 Section A에 기록한다.
