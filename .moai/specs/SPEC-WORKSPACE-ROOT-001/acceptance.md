---
id: SPEC-WORKSPACE-ROOT-001
title: "워크스페이스 시작 폴더 — 수용 기준"
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

# SPEC-WORKSPACE-ROOT-001 — 수용 기준

---

## §D AC 행렬

| AC | 대응 요구사항 | 검증 방식 | 마일스톤 | 심각도 |
|---|---|---|---|---|
| AC-1 | R9 | 자동 (cargo test) | M1 | **차단** |
| AC-2 | R1, R3 | 자동 (cargo test) | M1 | 차단 |
| AC-3 | R5 | 자동 (cargo test) | M1 | 차단 |
| AC-4 | R6, R7 | 자동 (cargo test) | M1 | 차단 |
| AC-5 | R7 | 자동 (cargo test) | M1 | 차단 |
| AC-6 | R8 | 자동 (cargo test) | M1~M5 | 차단 |
| AC-7 | R2, R3, R4, R8, R10, R11 | 수동 E2E | M4 | 차단 |
| AC-8 | R5, R6, R7 (headless 간접) | 자동 (autotest.ts) | M5 | 차단 |
| AC-9 | 전체 (품질 게이트) | 자동 (clippy + build) | M5 | 차단 |
| AC-10a | R6 (경로 정규화 — 순수) | 자동 (cargo test) | M1 | 차단 |
| AC-10b | R6 (존재 검증) | 자동 (cargo test) | M1 | 차단 |
| AC-11 | spec.md §C 문서화 의무 | 반자동 (파일 존재 + grep) | **sync (§F.S)** | **차단 (sync 단계 한정)** |
| AC-12 | spec.md §C 컨트롤 API 경로 비노출 | 자동 (cargo test + grep) | M2 | 차단 |

**단계별 차단 범위**: AC-1 ~ AC-10b 및 AC-12는 **run 단계 종료**를 차단한다. AC-11은 run 단계를 차단하지 **않으며**(문서는 run 이후에 쓰인다), **sync 단계 종료 = SPEC 완료**를 차단한다. 즉 AC-11 미충족 상태로 `status: implemented`까지는 갈 수 있으나 `completed`로는 갈 수 없다. 상세는 §D.4 참조.

---

## §D.1 AC 상세

### AC-1 — v7 설정 파일 무손실 마이그레이션 (R9) **[최우선 RED]**

> **Given** `schemaVersion: 7`이고 워크스페이스·팬 트리·pane cwd·command를 갖춘 현실적인 설정 파일이 존재할 때
> **When** `config::migrate()`가 그 값을 파싱하면
> **Then** 결과 `Config`의 `schema_version == 8`이고, 워크스페이스가 하나도 소실되지 않으며, 모든 `Workspace.root_dir`이 `None`이다.

실행 가능한 검증:
```bash
cd src-tauri && cargo test v7_fixture_migrates_to_v8
```

추가로, 기존 v1 / v2 / v3 마이그레이션 테스트에 `root_dir.is_none()` 단언을 덧붙인다.

```bash
cd src-tauri && cargo test migrate
```

**이 AC가 본 SPEC에서 가장 중요하다.** `config.rs:28`의 레거시 arm(`1 | 2 | 3 | 4 | 5 | 6`)을 `1..=7`로 확장하지 않은 채 `CONFIG_SCHEMA_VERSION`을 8로 올리면, 기존 v7 설정이 `other =>` 오류 분기로 떨어지고 `lib.rs`가 파일을 `config.json.invalid`로 옮겨 **사용자 워크스페이스가 전량 소실된다**. 이 테스트는 M1의 첫 RED 테스트여야 한다.

### AC-2 — 저장·로드 왕복 보존 및 JSON 키 고정 (R1, R3)

> **Given** `root_dir`이 설정된 워크스페이스를 포함한 설정이 있을 때
> **When** `save_config` 후 `load_config`를 수행하면
> **Then** `root_dir` 값이 그대로 보존된다.
>
> **And Given** 동일 워크스페이스를 직렬화할 때
> **Then** `serde_json::to_value(&ws)["rootDir"]`가 설정된 경로와 일치한다 (camelCase 키 고정).
>
> **And Given** `root_dir`이 `None`인 워크스페이스를 직렬화할 때
> **Then** 결과 JSON 객체에 `rootDir` 키가 **존재하지 않는다** (`skip_serializing_if` 동작 증명).

```bash
cd src-tauri && cargo test root_dir_survives_save_load_roundtrip
```

### AC-3 — 설정 시 모든 팬 cwd 재작성 (R5)

> **Given** 팬 2개를 가진 워크스페이스가 있고
> **When** `set_root_dir(id, Some(<존재하는 폴더>))`를 호출하면
> **Then** 반환값이 `Ok(2)`이고, 두 `PaneLeaf.cwd`가 모두 새 루트와 같으며, `metas()[0].root_dir`이 새 루트와 일치한다.

```bash
cd src-tauri && cargo test set_root_dir_rewrites_leaf_cwds
```

### AC-4 — 잘못된 입력 거부 및 무변경 보장 (R6, R7)

> **Given** 팬 2개와 알려진 cwd 값을 가진 워크스페이스가 있고
> **When** `set_root_dir`에 존재하지 않는 경로를 전달하면
> **Then** `Err`를 반환하고, `root_dir`은 여전히 `None`이며, **두 팬의 cwd가 모두 변경되지 않는다** (validate-before-mutate 증명).
>
> **And When** 디렉터리가 아닌 파일 경로를 전달하면 동일하게 거부한다.
> **And When** 존재하지 않는 워크스페이스 id를 전달하면 `Err("workspace not found: …")`를 반환한다.
> **And When** 공백 문자열을 전달하면 `root_dir`이 `None`으로 해제된다.

```bash
cd src-tauri && cargo test set_root_dir_rejects_missing_folder
cd src-tauri && cargo test set_root_dir_rejects_file_path
cd src-tauri && cargo test set_root_dir_unknown_workspace_errors
cd src-tauri && cargo test set_root_dir_blank_clears
```

### AC-5 — 해제 시 기존 cwd 보존 (R7)

> **Given** `root_dir`이 설정되어 있고 팬 cwd가 그 루트로 재작성된 워크스페이스가 있을 때
> **When** `set_root_dir(id, None)`을 호출하면
> **Then** `root_dir`이 `None`이 되고, 모든 `PaneLeaf.cwd`는 **변경되지 않는다**.

```bash
cd src-tauri && cargo test set_root_dir_clear_keeps_existing_cwds
```

### AC-6 — 미설정 워크스페이스 무회귀 (R8)

> **Given** `root_dir`이 설정되지 않은 워크스페이스가 있을 때
> **When** 팬 세션이 스폰되면
> **Then** 셸이 `default_cwd()`(즉 `%USERPROFILE%`)에서 시작한다.
>
> **And Then** 본 SPEC 이전에 존재하던 전체 테스트 스위트가 통과한다.

```bash
cd src-tauri && cargo test
```

**적용 범위 주의 (R8 한정)**: 이 AC는 `root_dir`을 **한 번도 설정한 적 없는** 워크스페이스만 검증한다. 설정했다가 해제한 워크스페이스는 `root_dir == None`이지만 팬 cwd가 옛 루트로 남으므로 `default_cwd()`에서 시작하지 **않는다** — 그 상태의 정본은 R7이며 AC-5가 검증한다. 두 AC는 상보적이며 겹치지 않는다.

### AC-7 — 수동 E2E: 대화상자 → 재시작 지속성 (R2, R3, R4, R8, R10, R11)

> **Given** 앱이 실행 중이고 사이드바에 워크스페이스가 하나 이상 있을 때
> **When** 워크스페이스 항목을 우클릭하고 "Choose folder…"를 선택하면
> **Then** 네이티브 폴더 대화상자가 **앱 창 앞에** 표시된다.
>
> **When** 폴더를 선택하면
> **Then** 메뉴를 다시 열었을 때 그 경로가 표시되고 "Clear" 항목이 나타난다.
>
> **When** 앱을 **완전히 종료했다가 다시 실행**하면
> **Then** 그 워크스페이스의 모든 터미널이 선택한 폴더에서 열린다.
>
> **When** 대화상자를 취소하면
> **Then** 아무 변화도 발생하지 않는다 (상태 변화 없음, 토스트 없음).
>
> **And Given** `root_dir`이 없는 워크스페이스를 우클릭하면
> **Then** "Not set"이 표시되고 "Clear" 항목이 렌더링되지 않으며, 그 워크스페이스의 터미널은 여전히 `%USERPROFILE%`에서 열린다.

검증 방식: 수동. **"완전히 종료했다가 다시 실행"** 단계는 생략할 수 없다 — 이것이 R3(영속화)과 R4(팬 시작 위치)를 함께 증명하는 유일한 지점이다.

### AC-8 — headless E2E 스크립트 (R5, R6, R7 간접 검증)

> **Given** `src/autotest.ts`가 실행될 때
> **Then** `rootDirSet`, `rootDirRewritesPanes`, `rootDirRejectsMissing`, `rootDirCleared` 체크가 모두 통과하고 `report.ok == true`이다.

**제약 (반드시 지킬 것)**: autotest에서 **`pick_folder`를 호출하지 않는다.** 네이티브 모달이 headless 실행을 멈춘다. `set_workspace_root`를 직접 호출하고, 경로는 `C:\\Windows`를 사용한다 — 해당 파일이 line 499/531에서 이미 "존재가 보장된 경로"로 쓰고 있는 값이다.

새 체크는 최종 `report.ok` 결합(약 line 601)에 포함해야 한다. 포함하지 않으면 체크가 실패해도 `report.ok`가 `true`로 남는다.

### AC-9 — 품질 게이트

> **Given** M5까지 구현이 완료되었을 때
> **When** 품질 도구를 실행하면
> **Then** 신규 경고가 0이고 빌드가 성공한다.

```bash
cd src-tauri && cargo clippy -- -D warnings
npm run build
```

커버리지: 목표 85%, 커밋당 최소 80% (`quality.yaml`). **측정 방법 주의** — 현재 Rust 측에는 커버리지 도구(`cargo-llvm-cov` / `cargo-tarpaulin`)가 구성되어 있지 않다. 도구가 구성되어 있으면 그 수치를 근거로 삼고, 구성되어 있지 않으면 대체 증거는 "새로 추가된 모든 공개 함수(`normalize_root_dir_str`, `normalize_root_dir`, `set_root_dir`, `set_workspace_root`, `pick_folder`)가 단위 테스트를 보유하며 AC-1~AC-5, AC-10a, AC-10b, AC-12가 그 경로를 덮는다"이다. 이 대체 증거는 §D.3 잔여 위험에 기록한다.

### AC-10a — 경로 정규화 순수 함수 (R6) **[파일시스템 무관]**

> **Given** `normalize_root_dir_str`(순수 함수, `is_dir()` 호출 없음)에 다양한 경로 문자열을 전달할 때
> **Then**:
>
> | 입력 | 기대 출력 | 검증 대상 규칙 |
> |---|---|---|
> | `C:\work\` | `Some("C:\work")` | 후행 구분자 1개 제거 |
> | `C:\work/` | `Some("C:\work")` | `/`도 동일하게 제거 |
> | `C:\work` | `Some("C:\work")` | 후행 구분자 없으면 무변경 |
> | `C:\` | `Some("C:\")` | 3자 미만이 되므로 제거 안 함 |
> | `\\host\share\` | `Some("\\host\share\")` | **UNC 가드** — `\\` 접두면 제거 안 함 |
> | `\\host\share` | `Some("\\host\share")` | 후행 구분자 없으면 규칙 미발동 |
> | `"  "` (공백) | `None` | trim 후 빈 문자열 |
> | `""` | `None` | 빈 문자열 |

```bash
cd src-tauri && cargo test normalize_str_
```

**테스트 이름 규약 (필수)**: 이 AC의 테스트는 전부 `normalize_str_` 접두사를 쓴다(예: `normalize_str_strips_one_trailing_separator`, `normalize_str_keeps_drive_root`, `normalize_str_keeps_unc_prefix`, `normalize_str_blank_is_none`). AC-10b는 `normalize_validated_` 접두사를 쓴다. **두 접두사는 서로의 부분 문자열이 아니므로 필터가 상호 배타적이다** — `cargo test normalize_str_`는 AC-10b의 테스트를 절대 매치하지 않고, 그 역도 성립한다. 규약은 plan.md M1에도 기재되어 있다.

이 규약이 필요한 이유: `cargo test <필터>`는 부분 문자열 매칭이다. 이전 판의 `cargo test normalize_root_dir`는 `normalize_root_dir_str…`로 시작하는 AC-10a 테스트까지 함께 매치했으므로, **AC-10b의 테스트를 하나도 쓰지 않아도 명령이 exit 0으로 통과**했다. §D.4.1이 이 명령으로 게이트하므로 run 단계 종료 게이트가 AC-10b의 부재를 탐지하지 못했다 — D11이 지적한 공허성이 AC 레벨에서 재발한 형태다.

**이 AC가 파일시스템에 의존하지 않는 것이 핵심이다.** 이전 판은 `C:\work`와 `\\host\share`를 단일 함수(존재 검증 포함)로 검증하려 했으나, 두 경로 모두 이 머신에 실재하지 않아 항상 `Err`로 떨어져 **명세대로는 통과가 구조적으로 불가능**했다(plan-audit D2). 또한 UNC 가드는 실재하는 네트워크 공유 없이는 검증할 수 없었다(D11). 순수 함수 분리로 두 문제가 동시에 해소된다.

**UNC 가드 비공허성 확인**: `\\host\share\` 케이스는 가드를 삭제하면 `\\host\share`가 되어 **실패한다**. 즉 이 케이스는 가드를 실제로 검증한다. 후행 구분자가 없는 `\\host\share`만 있었다면 규칙 2 자체가 발동하지 않아 가드 삭제 후에도 통과하는 공허한 테스트였을 것이다 — 두 케이스를 쌍으로 두는 이유다.

### AC-10b — 존재 검증 계층 (R6) **[파일시스템 의존]**

> **Given** `normalize_root_dir`(순수 정규화 + `is_dir()` 검증)에 경로를 전달할 때
> **Then**:
>
> | 입력 | 기대 출력 |
> |---|---|
> | `C:\Windows\` | `Ok(Some("C:\Windows"))` — 정규화 + 존재 확인 통과 |
> | temp-dir로 생성한 경로 + 후행 `\` | `Ok(Some(<후행 구분자 제거된 경로>))` |
> | 존재하지 않는 경로 | `Err` |
> | 디렉터리가 아닌 파일 경로 | `Err` |
> | `None` / 공백 | `Ok(None)` — `is_dir()`에 도달하지 않음 |

```bash
cd src-tauri && cargo test normalize_validated_
```

**테스트 이름 규약 (필수)**: 이 AC의 테스트는 전부 `normalize_validated_` 접두사를 쓴다(예: `normalize_validated_accepts_existing_dir`, `normalize_validated_rejects_missing`, `normalize_validated_rejects_file_path`, `normalize_validated_none_is_ok_none`). AC-10a의 `normalize_str_` 필터와 상호 배타적이므로, 두 AC 중 하나라도 테스트가 비어 있으면 해당 `cargo test` 명령이 "0 tests"로 드러난다.

`C:\Windows`는 `src/autotest.ts:496-497`이 주석으로 "C:\Windows is guaranteed to exist"라 명시하고 line 499/531에서 이미 쓰고 있는 경로다. temp-dir 케이스는 기존 `config.rs` 테스트의 temp-dir 패턴(약 line 133)을 따른다.

### AC-11 — 프로젝트 문서화 의무 이행 (spec.md §C) **[sync 단계]**

> **Given** run 단계(M1~M5)가 완료되고 `/moai sync`가 §F.S 산출물을 작성했을 때
> **When** 아래 체크리스트를 검증하면
> **Then** 모든 항목이 충족된다.

근거: `docs/DEVELOPMENT.md` §9 「문서 규칙」(line 218-228). 이 프로젝트는 기능마다 ADR 1건과 동반 문서 5종 갱신을 의무화한다.

| # | 항목 | 검증 명령 / 방법 | §F.S |
|---|---|---|---|
| 11-a | `docs/ADR-013-*.md`가 존재한다 | `ls docs/ADR-013-*.md` → 정확히 1개 | S1 |
| 11-b | ADR-013이 필수 4개 절을 갖는다 | `grep -cE '^#{2,3} *(배경\|맥락\|결정\|트레이드오프\|테스트)' docs/ADR-013-*.md` → **4 이상** | S1 |
| 11-c | ADR-013이 `rfd` vs `tauri-plugin-dialog` 결정을 담는다 | `grep -c 'tauri-plugin-dialog' docs/ADR-013-*.md` → **1 이상** | S1 |
| 11-d | ADR-013이 한국어로 작성되었다 | 육안 확인. §9 line 220-222 | S1 |
| 11-e | `GUIDE-command-palette.md`에 두 팔레트 커맨드가 `### <제목>` 표제로 있다 | `grep -c '^### Workspace: Set root folder' docs/GUIDE-command-palette.md` → **1**, `grep -c '^### Workspace: Clear root folder' docs/GUIDE-command-palette.md` → **1** | S2 |
| 11-f | 두 항목이 각각 "하는 일 / 쓰는 법 / 원리" 3불릿을 갖는다 | `grep -A6 '^### Workspace: Set root folder' docs/GUIDE-command-palette.md \| grep -c '하는 일\|쓰는 법\|원리'` → **3**, Clear 항목도 동일하게 **3** | S2 |
| 11-f2 | 두 항목이 파일의 어휘(책상 / 칸)를 따른다 | `grep -A6 '^### Workspace: Set root folder' docs/GUIDE-command-palette.md \| grep -c '책상'` → **1 이상**. Clear 항목도 동일 | S2 |
| 11-g | `GUIDE-features-easy.md` §6 한 줄 사전에 항목이 추가되었다 | `grep -ci '시작 폴더' docs/GUIDE-features-easy.md` → **1 이상** | S3 |
| 11-h | `README.md`에 사용자 관점 요약이 추가되었다 | `grep -ci '시작 폴더' README.md` → **1 이상** | S4 |
| 11-i | `PLAN-M1-M2-roadmap.md`에 "구현 완료 + 날짜 + 요약"이 추가되었다 | `grep -c '구현 완료' docs/PLAN-M1-M2-roadmap.md`가 이전 대비 증가 | S5 |
| 11-j | `DEVELOPMENT.md` 상태 요약이 갱신되었다 | `grep -c 'schemaVersion \*\*8\*\*' docs/DEVELOPMENT.md` → **1**. 같은 줄 묶음의 Rust 테스트 개수 / autotest 검사 개수 / `ADR-001~013` 표기도 함께 갱신 | S6 |
| 11-k | `DEVELOPMENT.md` 모듈 지도·레시피·함정이 갱신되었다 | 육안 확인. §9 line 228 | S6 |
| 11-l | `ARCHITECTURE.md` 영속화 목록에 `rootDir`이 있다 | `grep -c 'rootDir' docs/ARCHITECTURE.md` → **1 이상** | S7 |

**11-b 표기 근거 (plan-audit D6)**: 정규식이 `배경`과 `맥락`을 **동의어로 허용**한다. 기존 ADR 표기가 갈리기 때문이다 — 실측(`grep -hoE '^## *[가-힣]+'`) 결과:

| ADR | 실제 절 | 완화 후 카운트 |
|---|---|---|
| ADR-010 | `배경` / `결정` / `테스트` | 3 — 트레이드오프 절이 실제로 없음 |
| ADR-011 | `맥락` / `결정` / `검토한 대안` / `트레이드오프` / `향후` | 3 — 테스트 절이 실제로 없음 |
| ADR-012 | `맥락` / `결정` / `트레이드오프` / `테스트` | **4 — 유일하게 4개 절 완비** |

따라서 **ADR-013의 구조 모델은 ADR-012**다(plan.md §F.S S1에도 기재). ADR-010/011이 4에 못 미치는 것은 정규식 결함이 아니라 그 문서들이 실제로 절을 빠뜨린 것이므로, 임계값 4는 유지한다 — `DEVELOPMENT.md:223-224`의 "배경 → 결정 → 트레이드오프 → 테스트" 규칙에 충실한 값이다.

**11-e 표기 근거**: `GUIDE-command-palette.md`는 팔레트 항목을 `### <팔레트 제목>` 형태로 그대로 표제화한다(실측: `### Workspace: New`, `### Pane: Split left/right`, `### Shell: Enable multiline in PowerShell (Ctrl+Enter)`). 따라서 `^### ` 앵커가 이 문서의 실제 관례와 정합한다. 제목의 말줄임표(`…`)는 grep 대상에서 제외해 인코딩 취약성을 피한다. 두 항목은 §2 「책상(워크스페이스) 다루기」 절에 들어간다.

**11-f 기계화 근거**: 이 문서는 커맨드마다 `- **하는 일**:` / `- **쓰는 법**:` / `- **원리**:` 3불릿 고정 구조를 쓴다. 실측으로 `Workspace: New`, `Pane: Toggle zoom`, `View: Toggle sidebar` 모두 3을 반환했다. 따라서 육안 확인이 아니라 grep으로 판정한다. **`-A6`을 쓰는 이유**: 불릿 본문이 줄바꿈된다. `Workspace: New`만 봐도 `쓰는 법`이 2줄, `원리`가 3줄로 감기므로 `-A4`로는 `원리`를 놓친다.

**11-f2 어휘 근거 (N6)**: 이 문서는 §2 도입부에서 어휘를 명시적으로 선언한다 — "**워크스페이스 = 책상**. 프로젝트마다 하나씩 두고, 각 책상에 칸을 여러 개 둡니다." 기존 모든 항목이 이 어휘를 지킨다(예: `Workspace: New`의 "빈 새 책상을 만듭니다"). 이 파일은 비개발자를 위해 쓰였으므로, 기술 용어로 쓴 항목은 11-e/11-f를 통과하면서도 **파일에서 유일하게 개발자 어휘를 쓰는 항목**이 된다. `GUIDE-features-easy.md`에도 동일 어휘 규약이 적용된다(11-g).

일괄 확인용 배치:

```bash
ls docs/ADR-013-*.md
grep -cE '^#{2,3} *(배경|맥락|결정|트레이드오프|테스트)' docs/ADR-013-*.md
grep -c 'tauri-plugin-dialog' docs/ADR-013-*.md
grep -c '^### Workspace: Set root folder' docs/GUIDE-command-palette.md
grep -c '^### Workspace: Clear root folder' docs/GUIDE-command-palette.md
grep -A6 '^### Workspace: Set root folder' docs/GUIDE-command-palette.md | grep -c '하는 일\|쓰는 법\|원리'
grep -A6 '^### Workspace: Clear root folder' docs/GUIDE-command-palette.md | grep -c '하는 일\|쓰는 법\|원리'
grep -A6 '^### Workspace: Set root folder' docs/GUIDE-command-palette.md | grep -c '책상'
grep -A6 '^### Workspace: Clear root folder' docs/GUIDE-command-palette.md | grep -c '책상'
grep -ci '시작 폴더' docs/GUIDE-features-easy.md README.md
grep -c 'schemaVersion \*\*8\*\*' docs/DEVELOPMENT.md
grep -c 'rootDir' docs/ARCHITECTURE.md
```

**차단 범위**: AC-11은 **run 단계를 차단하지 않는다** — §F.S 산출물은 정의상 run 이후에 작성된다. AC-11은 **sync 단계 종료(SPEC 완료)를 차단한다**. 14개 항목 중 12개가 기계 판정이며, 육안 확인은 **11-d(한국어 작성) / 11-k(모듈 지도 갱신)** 2건뿐이다 — §D.3 잔여 위험에 기록한다.

### AC-12 — 컨트롤 API에 시작 폴더가 노출되지 않는다 (spec.md §C)

> **Given** `WorkspaceMeta`에 `root_dir` 필드가 추가되고 `root_dir`이 설정된 워크스페이스가 존재할 때
> **When** 네임드 파이프 브로커가 `listWorkspaces`를 호출하면
> **Then** 응답 JSON의 어떤 항목에도 `rootDir` 키가 존재하지 않는다 — 브로커 표면은 본 SPEC 이전과 동일하다.
>
> **And Given** 같은 워크스페이스를 프론트엔드가 조회할 때
> **Then** `WorkspaceMeta.rootDir`은 정상적으로 전달된다 (R11이 이에 의존한다).

실행 가능한 검증:

```bash
# 1. 단위 테스트 — listWorkspaces payload에 rootDir 키가 없음을 단언
cd src-tauri && cargo test pipe_list_workspaces_omits_root_dir

# 2. 구조 확인 — listWorkspaces arm이 metas()를 그대로 직렬화하지 않음
grep -A4 '"listWorkspaces" =>' src-tauri/src/commands.rs
#    기대: to_value(store.metas()) 직접 호출이 아니라 root_dir을 제거한 매핑을 거친다
```

테스트는 `handle_pipe_method`의 `listWorkspaces` 반환값을 `serde_json::Value`로 받아 각 항목에 `rootDir` 키가 없음을 단언한다. 프론트엔드 경로(`metas()` 직접 호출)는 별도로 `root_dir`이 실려 있음을 단언해, "그냥 필드를 빼버린" 회귀와 구분한다.

이 AC는 spec.md §C의 「컨트롤 API 경로 비노출」 제약과 §E 비목표를 강제한다. 근거·기각 대안은 plan.md §A.4.2.

---

## §D.2 추적 행렬 (R ↔ AC)

| 요구사항 | 대응 AC | 커버 |
|---|---|---|
| R1 — 선택적 시작 폴더 보유 | AC-2 | 자동 |
| R2 — 네이티브 대화상자로 설정 | AC-7 | 수동 |
| R3 — 재시작 후에도 유지 | AC-2, AC-7 | 자동 + 수동 |
| R4 — 팬이 시작 폴더에서 열림 | AC-7 | 수동 |
| R5 — 설정 시 팬 cwd 일괄 재작성 | AC-3, AC-8 | 자동 |
| R6 — 존재하지 않는 경로 거부 | AC-4, AC-8, AC-10a, AC-10b | 자동 |
| R7 — 해제 시 cwd 보존 | AC-4, AC-5, AC-8 | 자동 |
| R8 — 한 번도 설정한 적 없는 워크스페이스 무회귀 | AC-6, AC-7 | 자동 + 수동 |
| R9 — v7 설정 무손실 마이그레이션 | AC-1 | 자동 |
| R10 — 대화상자 취소 시 무변경 | AC-7 | 수동 |
| R11 — 컨텍스트 메뉴 상태 표시 | AC-7 | 수동 |
| — (요구사항 아님) spec.md §C 문서화 의무 | AC-11 | 반자동 (sync 단계) |
| — (요구사항 아님) spec.md §C 컨트롤 API 경로 비노출 | AC-12 | 자동 |

미커버 요구사항: 없음. 모든 R이 최소 1개 AC에 대응한다.

AC-11과 AC-12는 의도적으로 R 번호에 대응하지 않는다. 이는 기능 동작이 아니라 **프로젝트 차원의 프로세스 제약**(`docs/DEVELOPMENT.md` §9)이므로, 요구사항이 아니라 spec.md §C 제약 표에 산다. R12를 신설해 요구사항으로 승격시키지 않은 이유는 그렇게 하면 "동작 요구사항"과 "프로세스 의무"가 같은 번호 공간에 섞이기 때문이다.

---

## §D.3 간접 검증과 잔여 위험

| 항목 | 이유 | 완화 |
|---|---|---|
| R2 / R4 / R10 / R11이 수동 E2E에만 의존 | 네이티브 폴더 대화상자는 headless로 구동할 수 없다. 실제 앱 재시작도 자동화 범위 밖이다 | AC-8이 `set_workspace_root`를 직접 호출해 그 아래 레이어(R5/R6/R7)를 자동 커버한다. 대화상자 자체는 얇은 래퍼(`pick_folder`)로 유지해 수동 검증 표면을 최소화한다 |
| 커버리지 수치 측정 도구 미구성 | Rust 측에 커버리지 러너가 없다 | AC-9의 대체 증거(신규 공개 함수 전수 단위 테스트) 사용. 도구 도입은 별도 SPEC 범위 |
| `rfd::set_parent`의 raw-window-handle 트레이트 불일치 가능성 | `rfd`와 `tauri`의 의존 버전에 좌우된다 | plan.md §B.4 — `set_parent` 제거가 허용 가능한 성능 저하. 대화상자는 여전히 동작하며 창 소유 관계만 잃는다 |
| 시작 폴더 설정 후 폴더가 삭제되는 경우 | `session.rs:463-466`의 조용한 폴백 | M5의 `warnings` 항목으로 노출. 크래시는 발생하지 않으며 팬은 정상적으로 열린다 |
| 사라진 루트를 사이드바가 정상처럼 표시 | 프론트엔드는 파일시스템을 stat 할 수 없다. 시각 표시를 구현하려면 메뉴 렌더마다 백엔드 왕복을 추가하거나 `WorkspaceMeta`에 유효성 플래그를 얹어야 한다 | spec.md §E에 비목표로 명시. 실제로 문제가 되는 시점(세션 스폰)에는 M5의 `warnings`가 도달한다. 사용자가 메뉴에서 잘못된 경로를 보고도 인지하지 못하는 창이 남는 것은 수용된 잔여 위험이다 |
| 루트 설정 후 "Save as template"으로 만든 템플릿이 이식 불가 | `workspace_as_template`(`commands.rs:952-969`)의 "cwds are kept literally" 계약 + R5의 cwd 일괄 재작성 | spec.md §E에 비목표로 명시. 기존 계약의 그대로의 귀결이며 본 SPEC이 도입한 결함이 아니다. `${repo}` 일반화는 `DEVELOPMENT.md` §10의 별도 로드맵 항목 |
| UNC 경로에서 `is_dir()`이 잠시 블로킹 | 네트워크 공유 응답 지연 | `pick_folder`가 `#[tauri::command(async)]`이므로 UI 스레드를 막지 않는다 |
| AC-11의 11-d / 11-k가 육안 확인 | "한국어로 쓰였는가", "모듈 지도가 실제로 갱신됐는가"는 grep으로 판정할 수 없다 | 나머지 12개 항목은 기계적으로 검증된다(11-f는 3불릿 고정 구조 덕분에, 11-f2는 어휘 선언 덕분에 기계화됨). 육안 2건은 sync-auditor 검토 대상으로 남긴다 |
| 11-f2가 `책상` 출현만 세고 어휘 일관성 전반을 판정하지는 않는다 | 어휘 적합성은 본질적으로 문체 판단이다 | 최소 기준(파일이 선언한 핵심 어휘가 실제로 쓰였는가)은 기계적으로 강제된다. 완전한 문체 일치는 sync-auditor 판단에 맡긴다 |
| 11-i의 grep이 "이전 대비 증가"라는 상대 기준 | `PLAN-M1-M2-roadmap.md`에 "구현 완료" 문자열이 이미 여러 번 등장한다 | sync 단계에서 해당 절을 직접 확인한다. 절대 카운트를 고정하면 다음 SPEC에서 곧바로 깨진다 |

---

## §D.4 종료 게이트 (Definition of Done)

두 단계로 나뉜다. 실행 주체가 다르므로 게이트도 다르다.

### D.4.1 run 단계 종료 게이트 (M5 완료 → sync 진입)

manager-develop 책임. 아래를 모두 만족해야 `/moai sync`로 넘어간다.

- [ ] AC-1 ~ AC-6, AC-8, AC-10a, AC-10b, AC-12 자동 테스트 전부 통과 (`cargo test` green)
- [ ] **AC-10a / AC-10b 양쪽에 실제 테스트가 존재함** — `cargo test normalize_str_`와 `cargo test normalize_validated_`를 **각각** 실행해 둘 다 "0 tests"가 아님을 확인한다. 두 필터는 상호 배타적이므로 한쪽이 비어 있으면 여기서 드러난다 (AC-10a/10b 이름 규약)
- [ ] AC-7 수동 E2E 완료 — **앱 완전 종료 후 재실행 단계를 포함**하여 확인
- [ ] AC-9 품질 게이트 통과 (`cargo clippy -- -D warnings`, `npm run build` 신규 경고 0)
- [ ] `config.rs:28`이 `1..=7` 범위 형태로 수정되어 있음 (plan.md §B.1)
- [ ] **`handle_pipe_method`의 `listWorkspaces` arm이 `root_dir`을 제거한 뒤 직렬화한다** — `grep -A4 '"listWorkspaces" =>' src-tauri/src/commands.rs`로 확인 (AC-12, spec.md §C)
- [ ] `Workspace { }` struct 리터럴 3곳(`state.rs:203`, `state.rs:255`, `config.rs:123`) 전부에 `root_dir: None` 추가됨
- [ ] `src-tauri/capabilities/default.json`이 `["core:default"]` 그대로임
- [ ] `package.json` / `package-lock.json`에 dialog 플러그인이 추가되지 않았음
- [ ] `commands.rs::split_pane`에 ADR-011 비재정의 주석이 있음
- [ ] 팔레트 항목 `ws.root` / `ws.root.clear`가 등록되어 있음
- [ ] spec.md §E의 9개 비목표가 어느 것도 침범되지 않았음

AC-11은 이 게이트에 포함되지 **않는다** — 문서는 아직 쓰이지 않았다.

### D.4.2 sync 단계 종료 게이트 (SPEC 완료)

manager-docs 책임. plan.md §F.S 산출 후 아래를 만족해야 `status: completed`로 전이한다.

- [ ] AC-11 체크리스트 11-a ~ 11-l (11-f2 포함, 총 14항목) 전부 충족 (§D.1 AC-11의 일괄 배치 실행)
- [ ] `docs/ADR-013-*.md` 신규 작성 — 한국어, 배경 → 결정 → 트레이드오프 → 테스트 4개 절, `rfd` vs `tauri-plugin-dialog` 결정 포함 (S1)
- [ ] `GUIDE-command-palette.md`에 두 커맨드의 하는 일/쓰는 법/원리 3불릿 설명, 책상/칸 어휘 준수 (S2)
- [ ] `GUIDE-features-easy.md` / `README.md` / `PLAN-M1-M2-roadmap.md` 갱신 (S3~S5)
- [ ] `DEVELOPMENT.md` 상태 요약(schemaVersion 8, 테스트 개수, ADR-001~013) + 모듈 지도/레시피/함정 갱신 (S6)
- [ ] `ARCHITECTURE.md` 영속화 목록에 `rootDir` 추가 (S7)
- [ ] D.4.1이 이미 통과된 상태임

---

## §D.5 전방 점검 (Forward-Looking Checks)

구현 중 아래 신호가 나타나면 즉시 멈추고 재검토한다.

- `cargo test`에서 **기존** 마이그레이션 테스트가 깨진다 → §B.1 함정을 밟았을 가능성. `config.rs:28`을 먼저 확인한다
- `set_root_dir` 오류 경로에서 팬 cwd가 이미 바뀌어 있다 → validate-before-mutate 순서가 뒤집혔다
- 폴더 선택 중 앱 UI가 멎는다 → `#[tauri::command(async)]` 누락 (§B.3)
- 표시되는 경로에 `\\?\`가 보인다 → `fs::canonicalize`가 어딘가에 들어갔다
- autotest가 응답하지 않는다 → `pick_folder`를 호출했다 (AC-8 제약 위반)
- `capabilities/default.json`에 디프가 생긴다 → 플러그인 경로로 흘러갔다 (plan.md §A.2 기각 대안)
- sync 단계에서 `ls docs/ADR-013-*.md`가 비어 있다 → §9 "기능 하나 = ADR 하나" 의무 미이행. `rfd` 결정이 plan.md에만 남아 SPEC과 함께 묻힌다
- ADR 번호가 013이 아닌 다른 값으로 잡힌다 → ADR-001~012가 이미 존재한다. 새 ADR이 그 사이에 끼어들었는지 `ls docs/ADR-*.md`로 확인
- `cargo test normalize_str_` 또는 `cargo test normalize_validated_`가 "0 tests"로 나온다 → 해당 AC의 테스트가 아직 없다. 이전처럼 한쪽 필터가 다른 쪽을 가려주지 않는다 (AC-10a/10b 이름 규약)
- `GUIDE-command-palette.md`의 새 항목이 "워크스페이스"·"페인" 같은 기술 용어를 쓴다 → 파일의 어휘(책상 / 칸)에서 이탈했다 (11-f2)
- 브로커 응답에 `rootDir`이 보인다 → `listWorkspaces` arm이 여전히 `metas()`를 그대로 직렬화한다 (AC-12, `DEVELOPMENT.md:207` 불변식 3 위반)
- `normalize_root_dir_str` 테스트에 `is_dir()`이 필요해진다 → 두 함수가 다시 합쳐졌다. 순수 계층은 파일시스템을 건드리지 않아야 한다 (plan.md §A.1)
- M1에서 예상 못 한 `Workspace { }` 컴파일 오류가 난다 → §C 표의 3곳 중 빠뜨린 곳이 있다
