---
id: SPEC-WORKSPACE-ROOT-001
title: "워크스페이스 시작 폴더 (workspace root folder)"
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

# SPEC-WORKSPACE-ROOT-001 — 워크스페이스 시작 폴더

## HISTORY

| 버전 | 날짜 | 작성자 | 변경 내용 |
|---|---|---|---|
| 0.1.0 | 2026-08-05 | manager-spec | 최초 작성 (plan-phase 아티팩트 생성) |
| 0.2.0 | 2026-08-05 | manager-spec | 문서화 의무 반영. `docs/DEVELOPMENT.md` §9(문서 규칙)가 요구하는 기능당 ADR 1건 + 동반 문서 5종 갱신을 §C 제약에 추가. plan.md는 §F.S(sync 단계 산출물) 신설로 run/sync 단계 분리, acceptance.md는 AC-11 추가 |
| 0.3.0 | 2026-08-05 | manager-spec | plan-audit iteration 1(0.72 FAIL) 대응. D1 `lib.rs` 오류 경로 메커니즘 정정, D2 정규화/검증 함수 분리, D3 컨트롤 API 경로 노출 차단 결정, D4 struct 리터럴 3곳 명시, D5 R8 한정, D8/D9 비목표 2건 추가, D13 패턴 라벨 정정 |
| 0.4.0 | 2026-08-05 | manager-spec | plan-audit iteration 2(0.85 PASS) 후 마무리 정리. N1 AC 표제의 R 매핑 정정(AC-4·AC-7), N2 AC-10a/10b 테스트 이름 규약으로 필터 상호 배타화, N3 AC-12 테스트를 M2 RED에 배치, N4 `rfd` 버전 표기 일관화, N5 11-f 기계화 + 11-f2 신설, N6 책상/칸 어휘 규약 명문화, N7 HISTORY 순서 정정 |

---

## §A 배경과 목적

### A.1 문제

현재 terminal-f의 모든 팬(pane)은 새 셸을 띄울 때 `model.rs::default_cwd()`가 돌려주는 경로(`%USERPROFILE%` → `$HOME` → `"."`)에서 시작한다. 사용자가 특정 프로젝트 폴더에서 작업하려면 워크스페이스를 열 때마다 매번 `cd`를 입력해야 한다. 워크스페이스는 이미 "작업 단위"를 표현하는 개념인데, 그 작업 단위가 어느 폴더에 속하는지를 표현할 방법이 없다.

### A.2 목적

워크스페이스마다 선택적인 **시작 폴더 (root folder)** 를 부여한다. 사이드바 우클릭 컨텍스트 메뉴에서 **네이티브 폴더 선택 대화상자 (native folder picker dialog)** 로 고르고, `config.json`에 저장되어 앱을 재시작해도 유지된다. 시작 폴더가 설정된 워크스페이스의 팬은 항상 그 폴더에서 셸을 연다. 설정하지 않으면 지금 동작이 그대로 유지된다.

### A.3 핵심 통찰 — 새로 만드는 것은 세 가지뿐

`session.rs::spawn_session(&self, workspace_id, pane_id, cwd: &str, command: Option<&str>)`는 이미 `cwd` 인자를 받아 `CommandBuilder::cwd()`에 넘긴다. 즉 "팬을 특정 폴더에서 연다"는 배관(plumbing)은 이미 존재한다. 실제로 새로 필요한 것은:

1. `Workspace`에 `root_dir` 필드 추가 (기존 `color` 필드와 동일한 영속화 경로)
2. `root_dir`을 설정하면서 해당 워크스페이스 트리의 모든 `PaneLeaf.cwd`를 다시 쓰는 커맨드
3. 네이티브 폴더 대화상자 커맨드 + 우클릭 메뉴 항목

---

## §B 요구사항 (GEARS)

GEARS 구조 키워드(`Where` / `While` / `When` / `shall`)와 코드 식별자는 영문 정본을 유지하고, 서술부는 한국어로 기술한다.

### R1 — 선택적 시작 폴더 보유 (Ubiquitous)

> The workspace **shall** 선택적 시작 폴더(`root_dir`) 값을 0개 또는 1개 보유한다.

`root_dir`은 `Option<String>`이며 기본값은 `None`이다. `Workspace.root`(팬 트리)와는 완전히 다른 개념이므로 혼동해서는 안 된다.

### R2 — 네이티브 대화상자로 설정 (Event-driven)

> **When** 사용자가 사이드바 워크스페이스 항목을 우클릭하고 "Choose folder…"를 선택하면, the app **shall** 네이티브 폴더 선택 대화상자를 앱 창 소유로 표시하고, 선택된 경로를 해당 워크스페이스의 `root_dir`로 설정한다.

### R3 — 재시작 후에도 유지 (Ubiquitous)

> The config store **shall** `root_dir`을 `config.json`의 워크스페이스 항목에 `rootDir` 키(camelCase)로 저장하고, 앱 재시작 시 복원한다.

### R4 — 팬이 시작 폴더에서 열림 (Capability gate)

> **Where** 워크스페이스에 `root_dir`이 설정되어 있으면, the session registry **shall** 해당 워크스페이스의 팬 셸을 그 폴더에서 시작한다.

R5에 의해 각 `PaneLeaf.cwd`가 이미 시작 폴더로 다시 쓰였으므로, `spawn_session`은 기존 경로 그대로 동작한다. 새로운 분기는 없다.

### R5 — 설정 시 기존 팬 cwd 일괄 재작성 (Event-driven)

> **When** `set_workspace_root`가 유효한 폴더로 호출되면, the workspace store **shall** 해당 워크스페이스 트리의 **모든** `PaneLeaf.cwd`를 그 폴더로 덮어쓰고, 재작성된 팬 개수를 반환한다.

살아 있는 PTY 세션은 건드리지 않는다. 실행 중인 셸의 작업 디렉터리는 외부에서 옮길 수 없기 때문이다. 결과적으로 **재시작 후** 모든 팬이 새 폴더에서 열린다. UI 토스트는 이 사실("재시작 후 적용")을 반드시 알려야 한다.

### R6 — 존재하지 않는 경로 거부 (Event-driven)

> **When** 존재하지 않는 경로 또는 디렉터리가 아닌 경로가 `set_workspace_root`에 전달되면, the workspace store **shall** 오류를 반환하고 워크스페이스 상태(`root_dir` 및 모든 `PaneLeaf.cwd`)를 변경하지 않은 채로 남긴다.

검증은 변형(mutation) **이전에** 수행한다(validate-before-mutate). 이는 `state.rs::reorder`가 이미 따르고 있는 계약과 동일하다.

### R7 — 해제 가능, 해제 시 cwd 보존 (Event-driven)

> **When** `set_workspace_root`가 `None`(또는 공백 문자열)으로 호출되면, the workspace store **shall** `root_dir`만 `None`으로 되돌리고 기존 `PaneLeaf.cwd` 값은 변경하지 않는다.

### R8 — 미설정 워크스페이스 무회귀 (Ubiquitous)

> The app **shall** `root_dir`을 **한 번도 설정한 적 없는** 워크스페이스에 대해 기존과 동일하게 동작한다 — 팬은 `default_cwd()`에서 시작하고, 기존 테스트 스위트는 전부 통과한다.

**한정 이유**: 루트를 설정했다가 R7로 해제한 워크스페이스는 `root_dir == None`이지만 팬 cwd는 R5가 재작성한 옛 루트로 남는다. 그 상태에서는 `default_cwd()`에서 시작하지 **않는다** — 이는 R7이 규정한 의도된 동작이다. R8을 "설정되지 않은"으로 쓰면 이 상태에 대해 거짓이 되므로 "한 번도 설정한 적 없는"으로 한정한다. 해제 후 상태의 정본은 R7이다.

### R9 — 기존 설정 파일 무손실 마이그레이션 (Event-driven)

> **When** `schemaVersion: 7` 설정 파일이 로드되면, the migration path **shall** 워크스페이스 손실 없이 새 스키마 버전으로 마이그레이션하고 `root_dir`을 `None`으로 채운다.

이 요구사항은 본 SPEC에서 **가장 위험도가 높다**. 상세 근거는 plan.md §B.1을 참조한다.

### R10 — 대화상자 취소 시 무변경 (Event-driven)

> **When** 사용자가 폴더 선택 대화상자를 취소하면, the app **shall** 어떤 IPC 호출도 수행하지 않고 상태·토스트·재렌더 없이 조용히 종료한다.

### R11 — 컨텍스트 메뉴 상태 표시 (Capability gate)

> **Where** 우클릭한 워크스페이스에 `root_dir`이 설정되어 있으면, the sidebar menu **shall** 현재 경로(축약 표시, 전체 경로는 `title` 속성)와 "Clear" 항목을 함께 렌더링한다. 설정되어 있지 않으면 "Not set"을 표시하고 "Clear" 항목을 렌더링하지 않는다.

---

## §C 제약 (Constraints)

| 구분 | 제약 |
|---|---|
| 아키텍처 | 백엔드 소유 상태(backend-owned state). 프론트엔드는 워크스페이스 상태를 로컬 변형하지 않고, 모든 변경은 `#[tauri::command]` 왕복 후 반환된 새 상태로 갱신한다. |
| 영속화 | `%APPDATA%\com.terminalf.app\config.json` 단일 파일. `config.rs::save_config`의 tmp + rename 원자적 쓰기 경로를 그대로 사용한다. |
| 대화상자 | WebView2 네이티브 대화상자는 이벤트 루프를 블로킹한다(`src/modal.ts` 헤더 참조). 폴더 선택 커맨드는 UI 스레드를 막지 않아야 한다. |
| 권한 | `src-tauri/capabilities/default.json`은 `["core:default"]`로 유지한다. 새 permission identifier를 추가하지 않는다. |
| 경로 정규화 | `fs::canonicalize`를 호출하지 않는다. Windows에서 `\\?\` 확장 길이 접두사가 붙어 UI와 `CommandBuilder::cwd`로 새어 나간다. |
| 개발 방법론 | TDD (RED → GREEN → REFACTOR). `quality.yaml constitution.development_mode: tdd`. |
| 커버리지 | 목표 85%, 커밋당 최소 80%. |
| 워크스페이스 상한 | soft 8 / hard 16 (ADR-005). `WorkspaceMeta`에 필드를 하나 더 얹는 비용은 최대 16개로 상한이 있다. |
| **문서화 의무** | `docs/DEVELOPMENT.md` §9(문서 규칙, line 218-228)가 기능당 **ADR 1건 신규 작성**(`docs/ADR-XXX-*.md`, 배경 → 결정 → 트레이드오프 → 테스트 구조, 실패 후 재설계 경위 포함)과 동반 문서 5종(`PLAN-M1-M2-roadmap.md`, `README.md`, `GUIDE-features-easy.md`, `GUIDE-command-palette.md`, `DEVELOPMENT.md`) 갱신을 의무화한다. 본 SPEC은 팔레트 커맨드 2개를 추가하므로 `GUIDE-command-palette.md` 갱신이 **명시적으로 발동**된다(같은 문서 line 118-122 「새 팔레트 커맨드」 레시피). 다음 빈 번호는 **ADR-013**이다(ADR-001~012 존재). 산출은 sync 단계 책임이며 검증 기준은 acceptance.md AC-11이다. |
| 문서 언어 | `DEVELOPMENT.md` §9 line 220-222 — **모든 문서는 한국어로 작성한다**(코드 식별자·파일명·명령어는 영어 유지). ADR-013도 한국어로 쓴다. |
| **컨트롤 API 경로 비노출** | `commands.rs::handle_pipe_method`의 `listWorkspaces` arm이 `store.metas()`를 그대로 직렬화한다. `WorkspaceMeta`에 `root_dir`을 추가하면 인증된 외부 브로커가 사용자 절대 경로를 opt-in 없이 읽게 된다. 이 프로젝트는 `model.rs:42-44`에서 `allow_observe`를 기본 `false`로 두고 "not exposed to external brokers unless enabled"를 명문화했고, `DEVELOPMENT.md:207` 불변식 3이 게이트 우회를 금지한다. 따라서 **파이프 경계에서 `root_dir`을 제거**한다 — 브로커 표면은 오늘과 동일하게 유지된다. 근거와 기각 대안은 plan.md §A.4, 검증은 acceptance.md AC-12. |
| 스키마 불변식 | `DEVELOPMENT.md` §8 불변식 4(line 211-212) — "필드 추가마다 버전 +1과 fixture 테스트. 구버전 config가 조용히 깨지는 일이 없어야 함." 본 SPEC의 R9/AC-1이 이 불변식의 직접 이행이다. |

---

## §D 부록 — 엣지 케이스

| 케이스 | 기대 동작 |
|---|---|
| 대화상자 취소 | `Ok(None)` → 프론트엔드 조기 반환. IPC·토스트·재렌더 없음 (R10) |
| 설정 후 폴더가 삭제됨 | `session.rs`의 `if cwd_path.is_dir()` 블록이 조용히 건너뛰어 프로세스 cwd를 상속. 팬은 정상으로 열리고 크래시 없음. M5에서 `warnings` 항목을 추가해 이 폴백을 사용자에게 노출한다 |
| 존재하지 않는 경로 | `normalize_root_dir`이 변형 이전에 거부 (R6) |
| UNC / 네트워크 경로 | `is_dir()`이 동작한다(공유가 불통이면 잠시 블로킹될 수 있으나 `async` 커맨드이므로 허용 범위). 후행 구분자 제거는 `\\` 접두 경로를 건너뛴다. `canonicalize`를 하지 않으므로 UNC 형태가 `CommandBuilder::cwd`까지 보존된다 |
| 공백 포함 경로 | `PathBuf`로 `CommandBuilder::cwd`에 전달되며 셸 문자열 보간을 거치지 않는다 → 인용부호 문제 없음. `shortPath`는 표시 전용 |
| 비ASCII / 260자 초과 경로 | `to_string_lossy` 사용, JSON은 처음부터 끝까지 UTF-8. `\\?\` 접두사를 붙이지 않으므로 Windows 장경로 규칙은 현재 `default_cwd`와 동일하게 적용된다 |
| 팬이 살아 있는 상태에서 루트 설정 | 저장된 cwd는 재작성되고, 실행 중 PTY는 실제 cwd를 유지한다. 토스트가 "재시작 후 적용"을 명시한다. 이후 `split_pane`은 여전히 live cwd를 상속한다 (ADR-011) |
| 대화상자가 열린 사이 워크스페이스 삭제 | `set_root_dir`이 변형 이전에 `Err("workspace not found: …")` 반환 |
| 루트 설정 후 "Save as template" | `commands.rs:952-969` `workspace_as_template`은 주석대로 "cwds are kept literally" — 팬 cwd를 그대로 템플릿에 담는다. R5로 모든 팬 cwd가 동일한 절대 경로가 된 뒤 템플릿을 저장하면 그 절대 경로가 고정된 이식 불가 템플릿이 생성된다. 이는 `workspace_as_template`의 기존 계약과 일관된 동작이며 본 SPEC은 이를 변경하지 않는다 — §E 비목표 참조 |
| 브로커가 `listWorkspaces` 호출 | `WorkspaceMeta`에 `root_dir`이 있어도 파이프 경계에서 제거되어 브로커 응답은 오늘과 동일하다 (§C 컨트롤 API 경로 비노출) |
| 루트 설정과 activity poll 동시 발생 | `set_root_dir`이 트리 순회 전체 구간 동안 store 락을 보유하고, `persist`는 락 해제 후 실행. `set_workspace_color`와 동일한 락 순서(store → registry, 역전 금지) |

---

## §E 범위 제외 (Exclusions)

본 SPEC이 **의도적으로 만들지 않는** 것들이다. 이 항목들은 미래의 독자가 "버그"로 오인해 고치지 않도록 명시적으로 기록한다.

### Out of Scope — split_pane의 live cwd 상속 변경

- `commands.rs::split_pane`은 `registry.pane_live_cwd()`를 저장된 `leaf.cwd`보다 우선한다. 이 동작(ADR-011)은 **변경하지 않는다**.
- 시작 폴더는 이 상속을 재정의(override)하지 않는다. 사용자가 셸에서 `cd`로 이동한 뒤 팬을 분할하면, 새 팬은 시작 폴더가 아니라 현재 작업 위치에서 열리는 것이 의도된 동작이다.

### Out of Scope — 워크스페이스 생성 흐름 변경

- 새 워크스페이스를 만들 때 폴더를 묻지 않는다. 생성 흐름은 지금 그대로다.
- 시작 폴더는 생성 후 사이드바 우클릭으로 설정한다.

### Out of Scope — 템플릿의 루트 폴더 유도

- `apply_template` → `create_with_root` 경로는 `root_dir: None`만 추가한다.
- 템플릿에서 루트 폴더를 추론하지 않는다. 템플릿은 여러 디렉터리에 걸칠 수 있으며, 추론은 방금 만든 팬 cwd들을 조용히 덮어쓰게 된다.

### Out of Scope — 실행 중 셸의 재배치

- 시작 폴더를 바꿔도 이미 실행 중인 PTY 세션은 이동시키지 않는다.
- 실행 중인 셸의 작업 디렉터리를 외부에서 옮기는 것은 신뢰할 수 있는 방법이 없다.

### Out of Scope — tauri-plugin-dialog 도입

- `tauri-plugin-dialog` / `@tauri-apps/plugin-dialog`는 채택하지 않는다.
- 근거와 기각된 대안의 상세 비교는 plan.md §A.2에 기록한다.

### Out of Scope — 컨트롤 API에 시작 폴더 노출

- 네임드 파이프 컨트롤 API(`listWorkspaces`)에 `rootDir`을 노출하지 않는다. 브로커 응답은 본 SPEC 이전과 동일하다.
- 브로커에 루트 경로를 넘기는 opt-in 게이트(`allow_observe` 계열)를 새로 도입하지 않는다 — 범위 확대이며 현재 필요로 하는 소비자가 없다.

### Out of Scope — 사라진 루트 폴더의 사이드바 시각 표시

- 설정된 루트 폴더가 나중에 삭제되어도 사이드바 컨텍스트 메뉴는 저장된 경로를 그대로 표시한다. 경고 아이콘·흐린 처리 등 시각적 stale 표시는 만들지 않는다.
- 이유: 프론트엔드는 파일시스템을 stat 할 수 없으므로, 시각 표시를 구현하려면 메뉴를 열 때마다 백엔드 왕복을 추가하거나 `WorkspaceMeta`에 유효성 플래그를 새로 얹어야 한다. 두 방법 모두 Tier M 범위를 넘는다.
- 대신 M5가 `ensure_sessions`의 기존 `warnings` 경로로 이 상태를 노출한다 — 실제로 문제가 되는 시점(세션 스폰)에 사용자에게 도달한다. 이 판단은 acceptance.md §D.3에 잔여 위험으로 기록한다.

### Out of Scope — 워크스페이스 → 템플릿 방향의 cwd 일반화

- "Save as template"(`workspace_as_template`) 시 팬 cwd를 `${repo}` 등으로 일반화하지 않는다. 기존의 "cwds are kept literally" 계약을 유지한다.
- 결과적으로 루트가 설정된 워크스페이스에서 만든 템플릿은 그 절대 경로에 묶인다. 이는 본 SPEC이 도입한 결함이 아니라 기존 계약의 그대로의 귀결이다.
- `${repo}` 자동 일반화는 `DEVELOPMENT.md` §10 line 236이 이미 별도 로드맵 항목("템플릿 UX")으로 올려둔 작업이다.

### Out of Scope — 팬 단위 시작 폴더

- 시작 폴더는 워크스페이스 단위 개념이다. 개별 팬마다 다른 시작 폴더를 지정하는 기능은 포함하지 않는다.

---

## §F 참조

- `.moai/specs/SPEC-WORKSPACE-ROOT-001/plan.md` — 구현 계획, 마일스톤, 기술 결정
- `.moai/specs/SPEC-WORKSPACE-ROOT-001/acceptance.md` — 수용 기준, Given-When-Then
- `docs/ARCHITECTURE.md` — 영속화 대상 목록 (sync 단계에서 갱신)
- `docs/DEVELOPMENT.md` — schemaVersion 기술 및 스키마 변경 절차 체크리스트
