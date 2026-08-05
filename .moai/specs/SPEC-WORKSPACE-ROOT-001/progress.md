---
id: SPEC-WORKSPACE-ROOT-001
title: "워크스페이스 시작 폴더 — 진행 기록"
version: "0.4.0"
status: in-progress
created: 2026-08-05
updated: 2026-08-05
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

_M3~M5 및 run-phase 완료 신호(§E.3)는 후속 delegation에서 기록._

## §E.3 Run-phase Audit-Ready Signal

_<pending run-phase>_

## §E.4 Sync-phase Audit-Ready Signal

_<pending sync-phase>_

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
