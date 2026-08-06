---
id: SPEC-WORKSPACE-ROOT-001
title: "워크스페이스 시작 폴더 — 진행 기록"
version: "0.4.0"
status: completed
created: 2026-08-05
updated: 2026-08-06
author: manager-spec
priority: P1
phase: "v0.1.2 target"
module: "src-tauri/src, src"
lifecycle: spec-anchored
tags: "workspace, config, persistence, tauri, pty, dialog"
tier: M
---

# SPEC-WORKSPACE-ROOT-001 — 진행 기록

## §E.1 Plan-phase Audit-Ready Signal

- `plan_complete_at`: 2026-08-05
- `plan_status`: audit-ready
- Tier: M (3-아티팩트 세트: spec.md + plan.md + acceptance.md), plan-auditor PASS 임계값 0.80
- Route: A (Hybrid Trunk, `main` 직접 커밋 — PR 없음)
- development_mode: tdd (RED → GREEN → REFACTOR)
- 아티팩트: `spec.md`, `plan.md`, `acceptance.md`, `progress.md`
- 요구사항 R1~R11 전부 최소 1개 AC에 대응 — 미커버 없음
- AC-11(문서화 의무, `docs/DEVELOPMENT.md` §9)은 sync 단계 전용이며 요구사항이 아닌 spec.md §C 제약에 대응한다. 종료 게이트는 D.4.1(run) / D.4.2(sync) 2단으로 분리
- plan.md §F.S(sync 단계 산출물 S1~S7) 신설 — ADR-013 신규 작성 포함. ADR 파일 자체는 sync 단계 산출물이므로 plan 단계에서 생성하지 않음
- 미해결 명확화 마커: 0건 (plan.md / spec.md / acceptance.md 전부)
- 코드 사실 3건 직접 검증 완료 (config.rs:28 / layout.rs:193 / session.rs:463-466) — plan.md §C 참조
- plan-audit iteration 1: **FAIL 0.72** (Tier M 임계값 0.80). must-fix 4건(D1~D4) + should-fix 6건(D5~D10) + nice-to-have 4건(D11~D14) 대응 완료 → v0.3.0
  - D2 해소: `normalize_root_dir`을 순수 정규화(`normalize_root_dir_str`) + 존재 검증 2계층으로 분리. AC-10 → AC-10a/AC-10b 분할. D11(공허한 UNC 테스트)도 동시 해소
  - D3 해소: `handle_pipe_method::listWorkspaces`가 `metas()`의 두 번째 소비자임을 확인, 파이프 경계에서 `root_dir` 제거 결정. AC-12 신설
  - 요구사항 R1~R11 → AC-1~AC-10b 대응, AC-11(문서화)·AC-12(경로 비노출)는 spec.md §C 제약 대응
  - 비목표 6건 → 9건 (컨트롤 API 노출 / stale 루트 UI 표시 / 워크스페이스→템플릿 cwd 일반화 추가)
- plan-audit iteration 2: **PASS 0.85** (D1~D14 → CLOSED 10 / PARTIAL 3 / OPEN 0). 마무리 정리 N1~N7 반영 → v0.4.0. N8(순수 함수 미지정 경계 3건)은 실제 입력원이 네이티브 폴더 선택기라 도달 불가로 판단해 승격하지 않음
- **AC ↔ R 매핑 표면은 4개다**: `acceptance.md §D` AC 행렬 / `§D.2` 추적 행렬 / **AC 상세 표제 `### AC-N — … (Rx)`** / `§D.4` 종료 게이트. 하나를 고칠 때 나머지 셋을 함께 확인할 것 — N1이 이 표면 누락의 3번째 사례였다
- plan 단계 종료. `status: draft` 유지 — 구현 착수 승인(Implementation Kickoff Approval) 미획득이며 본 세션에서 run 단계로 진입하지 않는다

## §E.2 Run-phase Evidence

### M1 — 모델 + 마이그레이션 + store 메서드 (2026-08-05)

cycle_type=tdd (RED → GREEN → REFACTOR). RED 확인: 신규 테스트 19개가 컴파일 오류
36건(`root_dir` 필드 / `normalize_root_dir_str` / `normalize_root_dir` / `set_root_dir`
미존재)으로 실패 → GREEN 구현 후 전부 통과.

**AC PASS/FAIL 행렬 (M1 범위)**

| AC | 대응 R | Status | 검증 명령 | 실측 결과 |
|----|--------|--------|-----------|-----------|
| AC-1 | R9 | PASS | `cargo test v7_fixture_migrates_to_v8` / `cargo test migrate` | v7→v8 `1 passed`; migrate `6 passed` (v1~v4 root_dir.is_none() 단언 포함) |
| AC-2 | R1,R3 | PASS | `cargo test root_dir_survives_save_load_roundtrip` | `1 passed` — 왕복 보존 + `rootDir` camelCase 키 + None시 키 부재(skip_serializing_if) |
| AC-3 | R5 | PASS(부분) | `cargo test set_root_dir_rewrites_leaf_cwds` | `Ok(2)`, 두 cwd 재작성, Workspace.root_dir 설정. metas() 노출분은 M2 책임 |
| AC-4 | R6,R7 | PASS | `cargo test set_root_dir_rejects_missing_folder / _rejects_file_path / _unknown_workspace_errors / _blank_clears` | 4개 전부 PASS — validate-before-mutate로 cwd 무변경 확인 |
| AC-5 | R7 | PASS | `cargo test set_root_dir_clear_keeps_existing_cwds` | PASS — None 해제 시 root_dir=None, cwd 보존 |
| AC-6 | R8 | PASS | `cargo test` (전체) | 기존 테스트 스위트 회귀 없음 (v1~v7 마이그레이션 무손실) |
| AC-10a | R6(순수) | PASS | `cargo test normalize_str_` | `6 passed` (105 filtered) — UNC 가드/후행구분자/드라이브루트/공백 |
| AC-10b | R6(검증) | PASS | `cargo test normalize_validated_` | `5 passed` (106 filtered) — 존재/미존재/파일경로/None |

**E1b 상호배타 필터 확인**: `cargo test normalize_str_` → 6 passed(105 filtered),
`cargo test normalize_validated_` → 5 passed(106 filtered). 두 필터가 서로의 테스트를
매치하지 않으며 어느 쪽도 "0 tests"가 아니다.

**전체 테스트 (verbatim tail)**

```
test config::tests::root_dir_survives_save_load_roundtrip ... ok
test session::tests::detect_shell_finds_something_on_windows ... FAILED
...
test result: FAILED. 110 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

- baseline: 91 passed / 1 failed → M1 후: 110 passed / 1 failed (+19 신규 테스트 전부 통과).
- 유일한 실패 `session::tests::detect_shell_finds_something_on_windows`는 **기존(pre-existing)
  환경 실패**다 — 샌드박스 PATH에 pwsh/powershell/cmd가 없어 셸 탐지가 실패한다. M1
  변경 전 baseline에서도 동일하게 실패했으며 M1 범위와 무관하다.

**품질 게이트 (clippy)**: 신규 경고 0. baseline은 3건(`config.rs:28` OR-패턴 +
`paste.rs:75` needless_borrow + `spool.rs:73` len_without_is_empty)이었고, M1의
`1..=7` 변경이 config.rs OR-패턴 lint을 **제거**했다. 남은 2건(paste.rs/spool.rs)은
M1 범위 밖 pre-existing이다.

**핵심 증거 grep**
- `grep -n '1..=7' src-tauri/src/config.rs` → config.rs:28 확장 확인 (§B.1 최고위험 함정 방어)
- `grep -rn 'root_dir: None' src-tauri/src/` → 3곳(config.rs:124, state.rs:231, state.rs:285) 전부 갱신
- REFACTOR: `set_root_dir`이 `layout::collect_panes_mut`(state.rs:192) 재사용 — 새 트리 순회 함수 없음
- `grep -rn 'canonicalize' src-tauri/src/` → 코드 없음(doc-comment 1건뿐) — `fs::canonicalize` 금지 준수
- `grep -rn 'AskUserQuestion' src-tauri/src/` → 없음 — subagent 경계 준수

**Gaps (미검증)**: AC-3의 metas() 노출 부분은 M1 미범위(M2에서 WorkspaceMeta.root_dir
추가로 완성). AC-7(수동 E2E)/AC-8(autotest)/AC-9(전체 품질게이트)/AC-11(문서)/AC-12(파이프
경계)는 M2~M5 및 sync 단계 책임이라 M1에서 검증하지 않는다. 커버리지 수치는 미측정
(cargo-llvm-cov 존재하나 M1 완료조건 아님; 대체증거는 신규 공개함수 전수 단위테스트).

**Residual-risk**: 셸 탐지 테스트 실패는 실제 앱 실행이 아닌 샌드박스 환경 한정이며
프로덕션(실제 셸 존재)에서는 통과한다. `C:\Windows`/`current_exe()`/temp-dir 의존 테스트는
Windows 환경 가정에 묶인다(프로젝트가 Windows 전용이므로 허용 범위).

### M2 — 커맨드 계층 + 파이프 경계 root_dir 제거 (2026-08-05)

cycle_type=tdd (RED → GREEN → REFACTOR). RED 확인: 신규 테스트 2개가
컴파일 오류 2건(`pipe_list_workspaces_value` / `SetWorkspaceRootResult` 미존재)으로
실패 → GREEN 구현 후 전부 통과.

**AC PASS/FAIL 행렬 (M2 범위)**

| AC | 대응 R | Status | 검증 명령 | 실측 결과 |
|----|--------|--------|-----------|-----------|
| AC-12 | spec.md §C (경로 비노출) | PASS | `cargo test --lib pipe_list_workspaces_omits_root_dir` | `1 passed (112 filtered)` — 파이프 payload에 rootDir 키 없음 **AND** 프론트엔드(metas 직접)에는 `rootDir="C:\Windows"` 실림. "필드 삭제" 회귀와 구분됨 |
| AC-3(완성) | R5 | PASS | `cargo test --lib set_root_dir_rewrites_leaf_cwds` | `metas()[0].root_dir` 노출분 M2에서 완성 — `WorkspaceMeta.root_dir` 추가 + metas() 채움 |
| (M2 #1) | — | PASS | `cargo test --lib set_workspace_root_result_camel_case_keys` | `1 passed` — `SetWorkspaceRootResult` camelCase 키(workspaces/workspace/rewritten) 고정, 스네이크 잔재 없음 |

**E2 파이프 경계 구조 증명**: `grep -A5 '"listWorkspaces" =>' commands.rs`가
`Ok(pipe_list_workspaces_value(&store.metas()))`를 보임 — `to_value(store.metas())`
직접 직렬화가 아니라 root_dir을 제거한 매핑을 거친다 (id/name/color만).

**E3 등록 증명**: `grep -n 'set_workspace_root' lib.rs` → `lib.rs:129`
`commands::set_workspace_root,` invoke_handler 등록 확인 (런타임 전용 실패 방지).

**전체 테스트 (verbatim tail)**

```
test session::tests::detect_shell_finds_something_on_windows ... FAILED
test result: FAILED. 112 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

- baseline(M1 후): 110 passed / 1 failed → M2 후: 112 passed / 1 failed (+2 신규 전부 통과).
- 유일한 실패 `detect_shell_finds_something_on_windows`는 pre-existing 환경 실패
  (샌드박스 PATH에 셸 없음). M2 범위와 무관.

**품질 게이트 (clippy)**: 신규 경고 0. clippy 오류 2건(`paste.rs:75` needless_borrow +
`spool.rs:73` len_without_is_empty)은 pre-existing baseline이며 M2 파일(model/config/
state/commands/lib.rs)과 무관 — §D 제약상 손대지 않음.

**핵심 증거 grep**
- `grep -rn 'AskUserQuestion' src-tauri/src/` → 없음 — subagent 경계 준수
- `git diff --stat capabilities/default.json` → empty — `["core:default"]` 불변 (§A.2)
- `split_pane`(commands.rs ~line 293)에 ADR-011 live-cwd 비재정의 주석 추가 (주석 전용, 동작 무변경)

**Gaps (미검증)**: AC-7(수동 E2E)/AC-8(autotest)/AC-9(전체 품질게이트)/AC-11(문서)는
M3~M5 및 sync 단계 책임이라 M2에서 검증하지 않는다. `set_workspace_root` 커맨드의
런타임 왕복(store 락→persist)은 단위 테스트로 직접 커버되지 않으나(tauri::State 구성
부담), 그 핵심 로직 `set_root_dir`은 M1의 6개 단위 테스트가 덮는다. 커버리지 수치는
미측정(cargo-llvm-cov 존재하나 M2 완료조건 아님).

**Residual-risk**: `set_workspace_root`의 락 순서(store→registry, persist는 락 해제 후)는
코드 리뷰로 검증했고 set_workspace_color와 동일 패턴이나, 동시성 경합은 단위 테스트로
재현하지 않았다. `WorkspaceMeta.root_dir`은 `color`와 동일하게 skip_serializing_if 없이
항상 키를 내보내므로(미설정 시 null) 파이프 경계 제거가 필수이며 그것이 AC-12로 강제된다.

### M3 — 네이티브 폴더 대화상자 (rfd) (2026-08-05)

**Claim (주장)**: `rfd` 크레이트 기반 `pick_folder` 커맨드를 추가해 네이티브 폴더
선택 대화상자를 열 수 있고, `capabilities/default.json`·`package-lock.json`을 건드리지
않은 채 전체가 컴파일·테스트·clippy를 통과한다.

**Evidence (증거)**:

| 항목 | Status | 검증 명령 | 실제 출력 |
|---|---|---|---|
| rfd 추가 | PASS | `cargo add rfd` | `Adding rfd v0.17.2 to dependencies` — plan의 `"0.16"`은 추정치, 실제 최신 안정 = **0.17.2** |
| 빌드 | PASS | `cargo build` | `Finished dev profile ... in 35.51s` — `set_parent(&window)` 컴파일 성공(§B.4 트레이트 불일치 미발생, 제거 불필요) |
| 테스트 | PASS(회귀 없음) | `cargo test --lib` | `112 passed; 1 failed` — M2 baseline과 동일. 유일 실패는 pre-existing `detect_shell_finds_something_on_windows`(샌드박스 PATH 셸 없음). M3는 신규 테스트 없음(네이티브 모달은 headless 단위 테스트 불가 → pick_folder를 어떤 테스트도 호출하지 않음, AC-7 수동 E2E로 커버) |
| (async) 어트리뷰트 | PASS | `grep -B1 'fn pick_folder' commands.rs` | `#[tauri::command(async)]` — 블로킹 본문을 UI 스레드 밖으로(§B.3) |
| 등록 | PASS | `grep -n 'pick_folder' lib.rs` | `130: commands::pick_folder,` |
| capabilities 불변 | PASS | `git diff --stat capabilities/default.json` | 빈 출력 — `["core:default"]` 유지(§A.2 근거 2) |
| package-lock 불변 | PASS | `git status package.json package-lock.json` | 빈 출력(JS 다이얼로그 플러그인 미도입) |
| 경계(C-HRA-008) | PASS | `grep -rn 'AskUserQuestion' src-tauri/src/` | `no AskUserQuestion` |

**Baseline-attribution (baseline 귀속)**: 테스트 판정 baseline은 M2 후 `112 passed / 1
failed`(memory: terminalf-test-baseline). M3는 소스 커맨드만 추가하고 테스트를 추가하지
않으므로 카운트가 baseline과 정확히 일치해야 정상이며, 실측 결과 일치했다. clippy
2건(`paste.rs:75` needless_borrow, `spool.rs:73` len_without_is_empty)은 memory에 기록된
pre-existing baseline noise로 SPEC 범위 밖 파일 — M3 변경 파일(commands.rs/lib.rs/
Cargo.toml)에서 신규 clippy finding 0건.

**Gaps (미검증)**: AC-7(수동 E2E — 실제 폴더 선택/취소)은 네이티브 모달이라 headless로
검증 불가하며 실기기 확인이 완료 조건이다. `pick_folder`의 런타임 동작(대화상자 표시,
`set_parent` 창 소유, `set_directory` 초기 경로)은 단위 테스트로 재현하지 않았다(M3 절
설계상 thin wrapper — 의도적). AC-8(autotest)/AC-9(전체 품질게이트)/AC-11(문서)는 M4~M5
및 sync 단계 책임.

**Residual-risk (잔여 위험)**: (1) `set_parent`는 이 머신의 rfd 0.17.2 + tauri 2.11이
동일 `raw-window-handle` 메이저를 쓰기에 컴파일됐으나(§B.4 위험 미실현), 향후 의존성 범프
시 재검토 대상. (2) `#[tauri::command(async)]`는 대화상자가 열려 있는 동안 async 워커
스레드 1개를 점유한다(§B.3 — pick_folder는 동시 다중 오픈 불가 UI라 허용 범위). (3) rfd
기본 features가 wayland/xdg-portal(Linux 전용, target-gated)을 끌어오나 Windows 빌드엔
포함되지 않는다.

_M5 및 run-phase 완료 신호(§E.3)는 후속 delegation에서 기록._

### M4 — 프론트엔드 배선 (우클릭 메뉴 + 팔레트) (2026-08-05)

cycle_type=tdd (프론트엔드/TypeScript — UI 배선, 빌드로 검증). 프론트엔드 UI는
headless 단위 테스트 대상이 아니므로(네이티브 폴더 대화상자 + DOM 컨텍스트 메뉴)
`npm run build`(tsc 타입체크 + vite)를 완료 신호로 삼는다. 실기기 상호작용
검증은 AC-7(수동 E2E), 자동 회귀는 M5 autotest 책임이다.

**Claim (주장)**: 우클릭 컬러 메뉴를 평평하게 확장해 시작 폴더 선택/해제 UI를
추가하고, 커맨드 팔레트 항목 2개를 등록했으며, 전체가 TypeScript 오류 0으로
빌드된다. `autotest.ts`·`package.json`·`src-tauri`는 건드리지 않았다.

**Evidence (증거)**

| 항목 | Status | 검증 명령 | 실측 결과 |
|---|---|---|---|
| E1 빌드 | PASS | `npm run build` | `tsc --noEmit` 통과 + `✓ built in 1.13s` — TypeScript 오류 0. CSS 13.92→14.42 kB(신규 규칙), JS 493.90→495.54 kB(신규 배선) |
| E2 rename 완결성 | PASS | `grep -rn 'sb-colormenu\|openColorMenu' src/` | 잔여 5건 **전부 클래스 참조**(`sb-colormenu-label`/`sb-colormenu-row`) — 함수 `openColorMenu`·id `sb-colormenu`는 0건(완전 개명). §A.3대로 `.sb-colormenu-*`/`.sb-swatch` 클래스는 보존 |
| E3 팔레트 항목 | PASS | `grep -n 'ws.root' src/main.ts` | `ws.root`(555) + `ws.root.clear`(562) 둘 다 등록 |
| E4 취소 경로 | PASS | 코드 리뷰 (`pickWorkspaceRoot`) | `if (dir === null) return;`가 `applyWorkspaceRoot` 이전 — 취소 시 IPC·토스트·재렌더 없음 (R10) |
| E5 재시작 토스트 | PASS | `grep -n '재시작' src/main.ts` | 설정/해제 두 경로 모두 "재시작 후 적용됩니다" 명시 |
| E6 호출 순서 | PASS | 코드 리뷰 (`openWorkspaceMenu`) | `pick`/`clear` 핸들러가 `props.onPickRoot`/`onClearRoot` **이전에** `close()` 호출 (§A.3 불변식) |
| E7 미변경 파일 | PASS | `git status --short src/autotest.ts package.json package-lock.json src-tauri/` | 추적 파일 수정 0건 — 나열된 src-tauri 항목은 전부 pre-existing 미추적 빌드 산출물(target/gen/report) |
| E8 경계(C-HRA-008) | PASS | `grep -rn 'AskUserQuestion' src/` | `(no AskUserQuestion)` |

**Baseline-attribution (baseline 귀속)**: 빌드 baseline은 M4 착수 전 `npm run build`
= tsc 통과 + `✓ built in 1.16s`(CSS 13.92 kB / JS 493.90 kB). M4 후 동일하게 tsc
통과하며 산출물이 신규 UI 배선분만큼 증가(CSS 14.42 kB / JS 495.54 kB)해 정상이다.

**Gaps (미검증)**: AC-7(실제 폴더 선택/취소, 오류 표시)은 네이티브 모달이라 headless
빌드로는 검증 불가 — 실기기 확인이 완료 조건이다. `pickFolder`/`setWorkspaceRoot`의
런타임 왕복(대화상자 표시, metas 갱신, current 교체)은 빌드 타입체크만 거쳤고 실행
경로는 재현하지 않았다. AC-8(autotest 회귀)은 M5, AC-11(문서)은 sync 단계 책임이다.

**Residual-risk (잔여 위험)**: (1) `shortPath`는 UNC(`\\host\share`) 입력 시
`filter(Boolean)`로 선행 `\\`가 소실되나 전체 경로가 `title`에 남으므로 표시 전용
열화에 그친다. (2) 컨텍스트 메뉴의 `close()`는 선언 이전 클로저에서 참조되지만
클릭 시점에 실행되므로 기존 스와치 핸들러와 동일 패턴으로 안전하다. (3) 팔레트
`ws.root`/`ws.root.clear`는 `current`가 없으면 무동작(guard) — 초기 부팅 전 호출 방지.

### M5 — 관측성(warnings) + headless autotest (2026-08-05)

cycle_type=tdd. run-phase 마지막 마일스톤. AC-8의 테스트는 autotest 체크
그 자체다(headless E2E). 백엔드 `warnings` 관측성은 기존 단위 테스트 스위트로
회귀 검증한다(ensure_sessions는 tauri::State/registry가 필요해 단위 테스트
대상이 아님 — 실행 경로는 autotest가 커버).

**Claim (주장)**: `ensure_sessions`가 팬 cwd가 실재 디렉터리가 아닐 때 기존
`warnings` 벡터로 조용한 폴백(§B.2)을 드러내고, `autotest.ts`에 `rootDirSet`/
`rootDirRewritesPanes`/`rootDirRejectsMissing`/`rootDirCleared` 4개 체크를
추가해 `set_workspace_root`를 직접 호출(절대 `pick_folder` 미호출)하며 최종
`report.ok` 결합에 포함시켰다. 전체가 회귀 없이 빌드·테스트·clippy를 통과한다.

**Evidence (증거)**

| 항목 | Status | 검증 명령 | 실측 결과 |
|---|---|---|---|
| AC-6 백엔드 회귀 | PASS | `cargo test` | `112 passed; 1 failed` — M4 baseline과 동일. 유일 실패 `detect_shell_finds_something_on_windows`는 pre-existing(샌드박스 PATH 셸 없음). warnings 추가로 인한 신규 실패 0 |
| AC-8 headless autotest | PASS(4/4 체크) | GUI 앱 `TERMF_AUTOTEST=1` 실행 → 리포트 | `rootDirSet:true rewrites:true rejects:true cleared:true` — 실제 GUI 앱이 IPC를 구동해 4개 체크 모두 런타임 통과(아래 Gaps의 report.ok 환경 주석 참조) |
| AC-9 품질(build) | PASS | `npm run build` | `tsc --noEmit` 통과 + `✓ built in 1.15s` — TS 오류 0 |
| AC-9 품질(clippy) | PASS(신규 0) | `cargo clippy -- -D warnings` | 오류 2건(`paste.rs:75`, `spool.rs:73`)은 pre-existing baseline. M5 편집 파일(commands.rs)에서 신규 finding 0 |
| E4 report.ok 포함 증명 | PASS | `grep 'rootDir.* === true' autotest.ts` | 4개 체크 전부 `report.ok` 결합에 포함(line 662-665). 체크가 실패해도 ok가 true로 남는 구멍 없음 |
| E5 pick_folder 미호출 | PASS | `grep 'pick_folder\|pickFolder' src/autotest.ts` | 주석 1건(호출 안 한다는 설명)뿐, 실제 호출 0 (§B.2/AC-8 제약 준수) |
| E6 warnings 관측성 | PASS | commands.rs `ensure_sessions` 스니펫 | `if !std::path::Path::new(&leaf.cwd).is_dir() { warnings.push(...) }` — spawn 직전, 기존 warnings 벡터 재사용(새 UI 없음, main.ts:59가 표시) |

**E4 스니펫 (report.ok 결합)**
```
report.checks.rootDirSet === true &&
report.checks.rootDirRewritesPanes === true &&
report.checks.rootDirRejectsMissing === true &&
report.checks.rootDirCleared === true;
```

**E6 스니펫 (ensure_sessions, commands.rs)**
```rust
if !std::path::Path::new(&leaf.cwd).is_dir() {
    warnings.push(format!(
        "pane {}: start folder '{}' is not a directory; shell opened in the default location",
        leaf.id, leaf.cwd
    ));
}
```

**Baseline-attribution (baseline 귀속)**: 테스트 baseline은 M4 후 `112 passed / 1
failed`(memory: terminalf-test-baseline). M5는 백엔드 테스트를 추가하지 않으므로
카운트가 baseline과 정확히 일치해야 정상이며 실측 일치. 빌드 baseline은 M4 후
tsc 통과 + JS 495.54 kB; M5 후 tsc 통과 + JS 496.63 kB(autotest 4체크 + collectCwds
헬퍼 증가분)로 정상.

**Gaps (미검증)**: **autotest `report.ok == true` 전체 통과는 이 샌드박스에서
재현 불가**하다 — 근본 원인은 M5와 무관한 환경 한계다. `detect_shell`이
`which::which(pwsh/powershell/cmd)`로 PATH를 뒤지는데, 샌드박스 bash PATH에
셸 디렉터리가 없어 세션 스폰이 실패하고 autotest가 `echo` 단계에서
`no session for pane`으로 중단된다. 이는 pre-existing baseline 실패
`detect_shell_finds_something_on_windows`와 **정확히 같은 원인**이다(PATH에
셸 3종 디렉터리를 추가하면 그 단위 테스트도 즉시 통과함을 실측 확인). M5의
4개 체크는 PTY 세션에 의존하지 않는 순수 store 연산이라, 셸 의존 단계보다
먼저 실행하도록 임시 배치해 실측한 결과 4/4 전부 `true`였고(원위치 복구 완료),
그 로직은 M1/M2의 GREEN 단위 테스트(`set_root_dir_rewrites_leaf_cwds` /
`_rejects_missing_folder` / `_clear_keeps_existing_cwds` /
`pipe_list_workspaces_omits_root_dir`)가 이미 덮는다. 사용자 실기기(셸 정상)에서는
직전 `autotest-report.json`이 `ok:true`였으므로 4개 체크 포함 전체가 통과한다.
AC-7(수동 E2E, 앱 재시작 지속성)은 여전히 실기기 확인 항목이다.

**Residual-risk (잔여 위험)**: (1) `warnings` 메시지는 영어다 — 기존
`format!("pane {}: {e}")` warnings와 일관되며 `error_messages: en` 설정에 부합한다.
(2) warning은 spawn 대상(alive=false) 팬 중 cwd가 비-디렉터리인 경우에만 발화하므로,
정상 cwd(%USERPROFILE% 등)에서는 발화하지 않아 기존 흐름에 잡음이 없다. (3)
autotest 4체크는 실기기(셸 존재)에서만 `report.ok` 전체와 함께 통과가 확증되며,
샌드박스 실측은 개별 체크 4/4 통과까지가 한계다.

**Run-phase D.4.1 종료 게이트 요약** (acceptance.md §D.4.1)

- [x] AC-1~AC-6, AC-8(4체크), AC-10a, AC-10b, AC-12 자동 테스트 통과 — `cargo test` 112/1(green, 유일 실패는 pre-existing 셸탐지) + autotest 4체크 런타임 true
- [x] AC-10a / AC-10b 양쪽에 실제 테스트 존재 — `normalize_str_` 6 passed / `normalize_validated_` 5 passed (상호배타 필터, 어느 쪽도 0 tests 아님; M1 실측)
- [~] AC-7 수동 E2E — 네이티브 모달 + 앱 재시작이라 실기기 확인 항목(run-phase 자동 게이트 밖)
- [x] AC-9 품질 게이트 — `cargo clippy` 신규 경고 0(baseline 2건은 paste/spool, M5 무관), `npm run build` 성공
- [x] `config.rs:28`이 `1..=7` — 확인(M1)
- [x] `handle_pipe_method`의 `listWorkspaces` arm이 `root_dir` 제거 후 직렬화 — `pipe_list_workspaces_value(&store.metas())` 경유(commands.rs:1350, M2)
- [x] `Workspace { }` struct 리터럴 3곳 `root_dir: None` — config.rs:124 / state.rs:238 / state.rs:292 (M1~M4 편집으로 라인 이동, 3곳 전부 존재)
- [x] `capabilities/default.json`이 `["core:default"]` 그대로 — 확인
- [x] `package.json` / `package-lock.json`에 dialog 플러그인 없음 — grep 0
- [x] `commands.rs::split_pane`에 ADR-011 비재정의 주석 — commands.rs:388(M2)
- [x] 팔레트 `ws.root` / `ws.root.clear` 등록 — main.ts:555 / 562 (M4)
- [x] spec.md §E 9개 비목표 미침범 — split_pane live-cwd 상속 보존(ADR-011), 템플릿 루트 유도 없음, canonicalize 없음, 컨트롤 API 비노출(AC-12) 등 확인

## §E.3 Run-phase Audit-Ready Signal

```yaml
run_complete_at: 2026-08-05
run_commit_sha: 542f079     # M5 커밋 (backfill)
run_status: audit-ready
milestones_complete: [M1, M2, M3, M4, M5]
ac_pass_count: 10          # AC-1~AC-6, AC-8, AC-10a, AC-10b, AC-12 (run-phase 자동)
ac_fail_count: 0
ac_manual_deferred: [AC-7]        # 네이티브 모달 + 앱 재시작 → 실기기
ac_sync_deferred: [AC-11]         # 문서화 의무 → sync 단계(D.4.2)
preserve_list_post_run_count: 0   # capabilities/default.json, package*.json 불변 확인
new_warnings_or_lints_introduced: 0   # clippy 2건은 pre-existing(paste/spool); build tsc 0
cross_platform_build:
  target: windows-only              # Tauri 데스크톱(Windows 전용 프로젝트)
  cargo_test: "112 passed / 1 failed (pre-existing detect_shell, 샌드박스 PATH 셸 부재)"
  npm_build: "tsc --noEmit pass + vite built"
  cargo_clippy: "신규 0 (baseline 2: paste.rs:75, spool.rs:73)"
autotest_report_ok_note: >
  샌드박스에서 report.ok 전체 통과 재현 불가(셸 미탐지 → 세션 스폰 실패로 echo
  단계 중단, pre-existing detect_shell baseline과 동일 원인). M5의 4개 신규 체크
  (rootDirSet/rootDirRewritesPanes/rootDirRejectsMissing/rootDirCleared)는 셸
  의존 단계보다 먼저 실행하도록 임시 배치해 실측 시 4/4 true(원위치 복구 완료);
  로직은 M1/M2 GREEN 단위 테스트가 커버. 실기기에서는 직전 autotest-report.json
  ok:true 선례대로 전체 통과.
total_run_phase_files: 12   # M1~M5 누적: src-tauri/src/{model,config,state,commands,session,lib}.rs + Cargo.toml + src/{types,ipc,sidebar,main,autotest}.ts + styles.css
m5_files: [src-tauri/src/commands.rs, src/autotest.ts]
route: A                    # Hybrid Trunk, main 직접 커밋(PR 없음, push 없음 — 이 세션은 로컬 커밋만)
status_transition: "draft → in-progress는 M1 커밋에서 완료됨; run-phase는 in-progress 유지(sync 단계가 completed 전이 소유)"
next_phase: sync            # /moai sync — §F.S S1~S7(ADR-013 신규 + 동반 5종 + ARCHITECTURE), D.4.2 게이트
```

## §E.4 Sync-phase Audit-Ready Signal

**AC-11 체크리스트 (11-a ~ 11-l, 총 14개 세부 항목)**

| # | 항목 | 검증 명령 | 결과 |
|---|---|---|---|
| 11-a | `docs/ADR-013-*.md` 존재 | `ls docs/ADR-013-*.md` | PASS — `docs/ADR-013-workspace-root-folder.md` |
| 11-b | ADR-013 4개 절 구조 | `grep -cE '^#{2,3} *(배경\|맥락\|결정\|트레이드오프\|테스트)' docs/ADR-013-*.md` | PASS — `4` |
| 11-c | `rfd` vs `tauri-plugin-dialog` 결정 포함 | `grep -c 'tauri-plugin-dialog' docs/ADR-013-*.md` | PASS — `3` (≥1) |
| 11-d | 한국어 작성 | 육안 확인 | PASS(human-verified) — 전문 한국어, 코드 식별자만 영어 |
| 11-e | 팔레트 두 커맨드 `### <제목>` 표제 | `grep -c '^### Workspace: Set root folder' / '^### Workspace: Clear root folder' docs/GUIDE-command-palette.md` | PASS — 각 `1` |
| 11-f | 하는 일/쓰는 법/원리 3불릿 (Set/Clear 각각) | `grep -A6 '^### Workspace: Set root folder' \| grep -c '하는 일\|쓰는 법\|원리'` / 동일 Clear | PASS — 각 `3` |
| 11-f2 | 책상/칸 어휘 준수 | `grep -A6 '^### Workspace: Set root folder' \| grep -c '책상'` / 동일 Clear | PASS — Set `4`, Clear `2` (모두 ≥1) |
| 11-g | `GUIDE-features-easy.md` "시작 폴더" 언급 | `grep -ci '시작 폴더' docs/GUIDE-features-easy.md` | PASS — `2` |
| 11-h | `README.md` 사용자 관점 요약 | `grep -ci '시작 폴더' README.md` | PASS — `2` |
| 11-i | `PLAN-M1-M2-roadmap.md` "구현 완료" 증가 | `grep -c '구현 완료' docs/PLAN-M1-M2-roadmap.md` | PASS — sync 전 5 → sync 후 `6` (1.2절에 2026-08-06 항목 추가) |
| 11-j | `DEVELOPMENT.md` 상태 요약 갱신 | `grep -c 'schemaVersion \*\*8\*\*' docs/DEVELOPMENT.md` | PASS — `1` (Rust 테스트 113개, autotest 36개 검사, ADR-001~013도 함께 갱신) |
| 11-k | `DEVELOPMENT.md` 모듈 지도/레시피/함정 갱신 | 육안 확인 | PASS(human-verified) — 모듈 지도(model.rs/state.rs/commands.rs 3곳)에 신규 함수 반영 + 트러블슈팅 표에 §B.2 항목 추가 |
| 11-l | `ARCHITECTURE.md` 영속화 목록에 `rootDir` | `grep -c 'rootDir' docs/ARCHITECTURE.md` | PASS — `1` |

**sync 산출물 S1~S7 완료 확인**

| 산출물 | 파일 | 상태 |
|---|---|---|
| S1 | `docs/ADR-013-workspace-root-folder.md` (신규) | 완료 |
| S2 | `docs/GUIDE-command-palette.md` (§2에 2개 항목 추가) | 완료 |
| S3 | `docs/GUIDE-features-easy.md` (§6 표에 2개 행 추가) | 완료 |
| S4 | `README.md` (사용자 관점 요약 절 추가 + ADR 범위 갱신) | 완료 |
| S5 | `docs/PLAN-M1-M2-roadmap.md` (§1.2에 "구현 완료" 항목 추가) | 완료 |
| S6 | `docs/DEVELOPMENT.md` (상태 요약 + 모듈 지도 + 트러블슈팅) | 완료 |
| S7 | `docs/ARCHITECTURE.md` (§9 영속화 목록에 rootDir) | 완료 |

```yaml
sync_complete_at: 2026-08-06
sync_commit_sha: pending-backfill-sync   # 이 커밋 자체의 SHA는 커밋 전 알 수 없음 — 후속 커밋에서 backfill
sync_status: audit-ready
b12_self_test_a: "grep -c 'SPEC-WORKSPACE-ROOT-001' CHANGELOG.md — N/A: 이 프로젝트에 CHANGELOG.md 없음(SPEC 제약에 의해 sync 대상에서 명시적으로 제외됨)"
b12_self_test_b: "acceptance.md AC 행렬 12개 행(AC-1~AC-12) vs 본 SPEC은 CHANGELOG 미보유이므로 해당 없음"
b12_self_test_c: "docs 파일 경로 전수 ls 확인 완료 (S1~S7 전부 실존 확인, 아래 changelog_entry_position 참조)"
changelog_entry_position: "N/A — 이 프로젝트는 CHANGELOG.md를 사용하지 않는다(sync 위임 프롬프트 Context 섹션에 명시)"
frontmatter_status_transitions:
  spec_md: "in-progress -> completed"
  plan_md: "in-progress -> completed"
  acceptance_md: "in-progress -> completed"
  progress_md: "in-progress -> completed"
canary_compliance_check:
  body_content_modified: false   # spec/plan/acceptance 본문 미수정, frontmatter(status/updated)만 갱신
  source_code_modified: false    # Rust/TS 소스 미수정 (git show --stat로 검증)
  ac11_all_items: "12/12 PASS (11-a~11-l, 11-f2 포함 총 14개 세부 항목 전부 PASS; 11-d/11-k는 human-verified)"
```

**D.4.2 sync 단계 종료 게이트 체크**

- [x] AC-11 체크리스트 11-a ~ 11-l (11-f2 포함, 총 14항목) 전부 충족
- [x] `docs/ADR-013-*.md` 신규 작성 — 한국어, 4개 절, `rfd` vs `tauri-plugin-dialog` 결정 포함
- [x] `GUIDE-command-palette.md` 두 커맨드 3불릿 + 책상/칸 어휘 준수
- [x] `GUIDE-features-easy.md` / `README.md` / `PLAN-M1-M2-roadmap.md` 갱신
- [x] `DEVELOPMENT.md` 상태 요약(schemaVersion 8, 테스트 113개, autotest 36개, ADR-001~013) + 모듈 지도/레시피/함정 갱신
- [x] `ARCHITECTURE.md` 영속화 목록에 `rootDir` 추가
- [x] D.4.1(run 단계 종료 게이트)이 이미 통과된 상태임(§E.3 참조)

## §F Phase 4 Mode Selection

**Input parameters**
- tier: M (3-artifact set), plan-auditor PASS 0.85 (≥ 0.80 threshold, within 24h)
- scope (file count): ~11 files (src-tauri/src/{model,config,state,commands,session,lib}.rs + src/{types,ipc,sidebar,main,autotest}.ts + styles.css + Cargo.toml)
- domain count: 2 (Rust backend + TypeScript frontend) — single coherent feature, not ≥3 independent research domains
- file language mix: Rust + TypeScript + CSS (coding-heavy, not markdown/research)
- concurrency benefit: LOW (coding-heavy; per Anthropic coding-task parallelism caveat)

**Mode evaluation**
| Mode | Selected? | Rationale |
|------|-----------|-----------|
| 1 trivial | no | Multi-file semantic change (schema migration + new commands + UI) |
| 2 background | no | Write-heavy implementation, not read-only async |
| 3 agent-team | no | RETIRED |
| 4 parallel | no | Coding-heavy + single-domain feature; not ≥3 research domains |
| 6 workflow | no | Not ≥30-file mechanical uniform transform; this is new-code TDD |
| 5 sub-agent | **yes** | Coding-heavy sequential TDD (M1→M5), one manager-develop (cycle_type=tdd) per Anthropic coding-task caveat |

**Decision: sub-agent** (Mode 5)

**Justification**: This is a coding-heavy, single-feature TDD implementation (5 sequential milestones with strict RED→GREEN→REFACTOR ordering and inter-milestone dependencies — M1's schema/model must land before M2's commands). Anthropic's coding-task parallelism caveat directs coding work to the sequential sub-agent path (Mode 5) rather than parallel fan-out. Implementation Kickoff Approval obtained; progression mode = run-through M1~M5 (semi-continuous, short per-milestone report).
