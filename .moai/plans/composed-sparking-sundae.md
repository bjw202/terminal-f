# /moai plan — 워크스페이스별 시작 폴더 SPEC 작성

## Context — 왜 이 문서들을 만드는가

terminal-f 에서 워크스페이스를 만들거나 앱을 재시작하면 터미널이 항상 기본 위치(`%USERPROFILE%`)에서 열립니다. 프로젝트별로 워크스페이스를 나눠 쓰는 방식에서는 매번 `cd` 를 다시 쳐야 합니다.

사이드바 우클릭 메뉴에는 이미 **Color label** 이 있고 그 값은 `config.json` 에 저장되어 재시작 후에도 유지됩니다. 같은 메뉴·같은 저장 경로에 **"시작 폴더"** 를 얹어, 네이티브 폴더 다이얼로그로 지정한 폴더에서 해당 워크스페이스의 터미널이 항상 열리도록 합니다. 지정하지 않은 워크스페이스는 지금과 완전히 동일하게 동작합니다.

**이번 단계에서 코드는 작성하지 않습니다.** `/moai plan` 은 문서 산출물만 만들고, 실제 구현은 이후 `/moai run SPEC-WORKSPACE-ROOT-001` 에서 진행합니다.

---

## 이번 단계의 산출물

| # | 경로 | 내용 |
|---|------|------|
| 1 | `.moai/project/product.md` | 제품 목적 · 대상 사용자 · 핵심 가치 |
| 1 | `.moai/project/structure.md` | 디렉터리 구조 · 모듈 경계 |
| 1 | `.moai/project/tech.md` | 기술 스택 · 빌드/테스트 체계 |
| 2 | `.moai/specs/SPEC-WORKSPACE-ROOT-001/spec.md` | GEARS 형식 요구사항 |
| 2 | `.moai/specs/SPEC-WORKSPACE-ROOT-001/plan.md` | 기술 설계 · 마일스톤 |
| 2 | `.moai/specs/SPEC-WORKSPACE-ROOT-001/acceptance.md` | 인수 조건 (AC) |
| 2 | `.moai/specs/SPEC-WORKSPACE-ROOT-001/progress.md` | 진행 기록 · 감사 신호 |

---

## 1단계 — 프로젝트 문서 (`.moai/project/`)

이 프로젝트에는 이미 `docs/ARCHITECTURE.md` 와 ADR 12건(ADR-002 ~ ADR-012)이 있습니다. 따라서 **처음부터 인터뷰로 캐묻지 않고**, 다음 순서로 진행합니다.

1. `Agent(Explore)` 로 `docs/`, `src/`, `src-tauri/`, `package.json`, `Cargo.toml` 을 읽어 **초안을 먼저 작성**
2. 코드가 답해줄 수 없는 항목만 **짧은 확인 인터뷰**(`AskUserQuestion` 1~2 라운드)
   - 제품의 대상 사용자 (본인 전용 / 공개 배포 / 팀 내부)
   - 앞으로의 방향 (지금 범위 유지 / 기능 확장 계획)
3. `manager-docs` 가 세 파일을 작성

> 담당: `Explore`(조사) → `manager-docs`(작성). 재사용 자료: `docs/ARCHITECTURE.md`, `docs/ADR-*.md`, `CLAUDE.md`

---

## 2단계 — SPEC-WORKSPACE-ROOT-001

### 확정 사항

| 항목 | 값 | 근거 |
|------|-----|------|
| SPEC ID | `SPEC-WORKSPACE-ROOT-001` | 사용자 선택 |
| Tier | **M** (Medium) | 약 11개 파일 · 300~1000 LOC → 3-artifact 세트, plan-auditor PASS 임계 0.80 |
| Route | **A** (Hybrid Trunk main-direct) | Tier M → PR 없이 `main` 직접 커밋 |
| 개발 방식 | **TDD** (RED-GREEN-REFACTOR) | `quality.yaml` `development_mode: tdd` |
| 커버리지 | 목표 85% / 커밋당 최소 80% | `quality.yaml` `test_coverage_target`, `min_coverage_per_commit` |
| 초기 status | `draft` | manager-spec 이 발행 |

### 사용자 확정 동작 결정 (SPEC 에 요구사항으로 명문화)

| 항목 | 결정 |
|------|------|
| 폴더 지정 시 기존 패널 | 저장된 패널의 `cwd` 도 함께 갱신 → **재시작 후 모든 패널이 지정 폴더에서 시작**. 실행 중인 셸은 옮길 수 없으므로 그대로 둠 |
| 패널 분할(split) | **기존 동작 유지** — 원래 패널의 현재 위치(live cwd, ADR-011) 상속. 시작 폴더가 이를 덮어쓰지 않음 |
| 새 워크스페이스 생성 | **흐름 변경 없음** — 즉시 생성되고, 폴더는 나중에 우클릭으로 지정 |
| 미지정 워크스페이스 | 현행 그대로 `default_cwd()` (`%USERPROFILE%`) |

### spec.md — 요구사항 (GEARS 형식으로 작성)

- **R1** 워크스페이스는 선택적 시작 폴더를 가질 수 있다
- **R2** 사이드바 우클릭 메뉴에서 네이티브 폴더 다이얼로그로 지정할 수 있다
- **R3** 지정한 폴더는 `config.json` 에 저장되어 앱 재시작 후에도 유지된다
- **R4** 시작 폴더가 설정된 워크스페이스의 패널은 그 폴더에서 셸을 시작한다
- **R5** 시작 폴더 설정 시, 해당 워크스페이스에 저장된 모든 패널의 `cwd` 가 함께 갱신된다
- **R6** 존재하지 않는 경로는 설정 시점에 거부되고, 워크스페이스 상태는 변경되지 않는다
- **R7** 시작 폴더를 해제할 수 있으며, 해제 시 기존 패널 `cwd` 는 유지된다
- **R8** 시작 폴더가 없는 워크스페이스는 기존 동작을 그대로 유지한다 (무손실)
- **R9** 기존 `schemaVersion: 7` 설정 파일은 손실 없이 이관된다
- **R10** 다이얼로그 취소 시 아무 변경도 일어나지 않는다

### plan.md — 기술 설계

**핵심 관찰**: 터미널을 띄우는 `session.rs::spawn_session(.., cwd, ..)` 는 **이미 폴더 인자를 받아 그 위치에서 셸을 실행**합니다. 지금은 그 값이 항상 기본 위치일 뿐입니다. 따라서 신규 구현은 세 가지로 압축됩니다.

1. `Workspace` 에 `rootDir` 필드 추가 + 저장 (색상 라벨과 동일 경로)
2. 폴더 지정 시 해당 워크스페이스의 모든 `PaneLeaf.cwd` 를 재작성하는 커맨드
3. 네이티브 폴더 다이얼로그 커맨드 + 우클릭 메뉴 항목

**다이얼로그 방식: `rfd` (Rust 커맨드) 채택 — Tauri plugin 아님**

이 코드베이스는 OS 연동을 전부 자체 Rust 커맨드로 구현합니다(`arboard` 클립보드, `png` 이미지 붙여넣기, `interprocess` 파이프). `tauri-plugin-dialog` 를 쓰면 npm 의존성이 추가되고 `capabilities/default.json` 에 `dialog:allow-open` 권한이 열려 **웹뷰에 범용 파일 다이얼로그 권한이 부여**됩니다. `rfd` 는 npm 변경 0, 권한 파일 변경 0, 검증이 Rust 쪽에서 끝납니다.

**마일스톤 (M1 → M5, TDD 사이클 단위)**

| M | 범위 | 주요 파일 |
|---|------|-----------|
| M1 | 데이터 모델 + 마이그레이션 (이 단계만으로 하위호환 보장) | `model.rs`, `config.rs`, `state.rs` |
| M2 | 상태 변경 커맨드 `set_workspace_root` | `commands.rs`, `lib.rs` |
| M3 | 네이티브 폴더 다이얼로그 `pick_folder` | `Cargo.toml`, `commands.rs`, `lib.rs` |
| M4 | 프론트엔드 (타입 · IPC · 우클릭 메뉴 · CSS) | `types.ts`, `ipc.ts`, `sidebar.ts`, `main.ts`, `styles.css` |
| M5 | 폴더 소실 경고 + 통합 테스트 + 문서 | `commands.rs`, `autotest.ts`, `docs/` |

**재사용하는 기존 코드 (신규 작성 없음)**

- `layout::collect_panes_mut` — `src-tauri/src/layout.rs:193` (패널 트리 순회)
- `session.rs::spawn_session` 의 `cwd` 인자 — `src-tauri/src/session.rs:464` (이미 완비)
- `commands.rs::persist()` — 저장 로직
- `state.rs::set_color()` — 신규 `set_root_dir` 의 구조적 템플릿
- `sidebar.ts::openColorMenu()` 의 바깥-클릭 닫기 핸들러 · `sidebarBusy` 가드
- `main.ts:59` 의 `warnings` 표시 경로 (신규 UI 불필요)
- `config.rs:133` 의 임시 디렉터리 테스트 패턴

### ⚠️ SPEC 에 반드시 명시할 함정

`src-tauri/src/config.rs:28` 의 마이그레이션 분기가 `1 | 2 | 3 | 4 | 5 | 6` 입니다.

`CONFIG_SCHEMA_VERSION` 을 7 → 8 로 올리면 **현재 사용 중인 v7 설정 파일이 `other` 분기로 떨어져 거부**되고, `lib.rs` 부팅 로직이 이를 `config.json.invalid` 로 밀어낸 뒤 **워크스페이스가 전부 초기화**됩니다.

→ 해당 줄을 `1..=7` 로 수정하고, 이를 검증하는 회귀 테스트 `v7_fixture_migrates_to_v8` 을 M1 의 RED 단계 첫 테스트로 배치합니다. plan.md 와 acceptance.md 양쪽에 기록합니다.

### acceptance.md — 인수 조건 (요구사항 ↔ AC 완전 매핑)

각 AC 는 실행 가능한 검증 명령을 동반합니다.

- **AC-1 (R9)** `cargo test v7_fixture_migrates_to_v8` 통과 — v7 설정이 v8 로 손실 없이 이관
- **AC-2 (R1/R3)** `root_dir_survives_save_load_roundtrip` 통과 — 저장→로드 보존, JSON 키가 `rootDir`(camelCase), 미설정 시 키 자체가 없음
- **AC-3 (R5)** `set_root_dir_rewrites_leaf_cwds` 통과 — 2분할 워크스페이스에서 모든 leaf `cwd` 갱신 + `rewritten == 2`
- **AC-4 (R6)** `set_root_dir_rejects_missing_folder` / `_rejects_file_path` 통과 — 실패 시 `root_dir` 와 leaf `cwd` 모두 불변 (검증-후-변경 보증)
- **AC-5 (R7)** `set_root_dir_clear_keeps_existing_cwds` 통과
- **AC-6 (R8)** 시작 폴더 미지정 워크스페이스가 `default_cwd()` 에서 열림 — 기존 테스트 전부 통과 (무손실)
- **AC-7 (R2/R4/R10)** 수동 E2E: 우클릭 → 폴더 선택 → **앱 완전 종료 후 재실행** → 해당 워크스페이스의 모든 터미널이 지정 폴더에서 열림. 다이얼로그 취소 시 무변화
- **AC-8** `src/autotest.ts` 의 `rootDirSet` / `rootDirRewritesPanes` / `rootDirRejectsMissing` / `rootDirCleared` 4개 체크 통과 + `report.ok == true`
- **AC-9** 커버리지 85% 이상 유지, `cargo clippy` · `npm run build` 무경고

### 엣지 케이스 (spec.md 부록으로 기록)

폴더 삭제 후 실행(조용한 폴백 → 경고로 가시화) · UNC/네트워크 경로 · 공백/비ASCII 경로 · 실행 중 패널 · 다이얼로그 도중 워크스페이스 삭제 · 설정과 활동 폴링의 동시성(lock 규율).

---

## 실행 흐름 (에이전트 위임)

```
1단계  Agent(Explore)      → docs/ src/ src-tauri/ 조사
       AskUserQuestion     → 코드가 답할 수 없는 항목만 짧게 확인
       Agent(manager-docs) → product.md / structure.md / tech.md 작성

2단계  Agent(manager-spec)   → spec.md / plan.md / acceptance.md / progress.md 작성
       Agent(plan-auditor)   → 독립 감사 (PASS 임계 0.80, 최대 3회 반복)
       → progress.md 에 plan_status: audit-ready 기록
```

---

## 검증 방법

**1) 문서 존재 · 구조 확인**
```
.moai/project/{product,structure,tech}.md          — 3개 파일
.moai/specs/SPEC-WORKSPACE-ROOT-001/               — 4개 파일
```

**2) SPEC 품질 게이트**
- `plan-auditor` 독립 감사 점수 **0.80 이상** (Tier M 임계)
- **요구사항 ↔ AC 완전 매핑**: R1~R10 각각이 최소 1개 AC 로 커버되는지 확인
- `[NEEDS CLARIFICATION]` 마커가 **0개** — 남아 있으면 구현 착수 전에 해소해야 함
- frontmatter 필수 필드 충족 (`id`, `title`, `status: draft`, `tier: M`, `created`, `updated`)

**3) 함정 반영 확인**
- `config.rs:28` 의 `1..=7` 수정이 plan.md 마일스톤 M1 과 acceptance.md AC-1 양쪽에 기록되어 있는지 직접 확인

**4) 사실 검증 (SPEC 내 코드 참조가 실제와 일치하는지)**
```
src-tauri/src/config.rs:28        → 1 | 2 | 3 | 4 | 5 | 6   (확인 완료)
src-tauri/src/layout.rs:193       → collect_panes_mut        (확인 완료)
src-tauri/src/session.rs:464      → cmd.cwd(cwd_path)        (확인 완료)
```

---

## 이후 흐름 (이번 승인 범위 밖)

SPEC 승인 후 → `/moai run SPEC-WORKSPACE-ROOT-001` 로 TDD 구현 → `/moai sync` 로 문서 동기화. 구현 착수 전에 **Implementation Kickoff Approval** 승인을 별도로 받습니다.
