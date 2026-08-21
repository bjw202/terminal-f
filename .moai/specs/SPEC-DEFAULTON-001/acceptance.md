# SPEC-DEFAULTON-001 — 수용 기준 (acceptance)

## §A 개요

기본 활성 UX 전환(Part A)과 팔레트 설치 커맨드 정리(Part B)를 **관찰 가능한 증거**로 판정한다. 모든 판정은 명령 실행 + verbatim 출력 기반이다(검증 주장 무결성 — 실행하지 않은 검증을 PASS로 보고하지 않는다).

**AC-ID 네임스페이스 규약**: 본 문서의 `AC-N`은 SPEC-DEFAULTON-001 고유 번호다.

**판정 성격의 구분(소급 기록 SPEC 특유)**:

- **AC-1~AC-5, AC-9, AC-14 (Part A 계약)**: 현재 작업 트리에서 재실행 가능한 **구조 grep 판정**이다 — 이미 구현된 코드의 계약을 고정하는 회귀 가드로 기능한다. `npm run build`·`cargo test`의 원본 관측은 구현 세션(2026-08-21) 기록이며, M1 완료 시 전체 트리 대상 표준 스위트로 재확인된다(Part A·B가 같은 트리에 있다). AC-14의 "정확히 1건" 기대는 Part B 제거 완료 후에만 성립한다(제거 전 baseline 4건).
- **AC-6~AC-8, AC-10~AC-13 (Part B)**: M1 완료 시점에 판정한다.

autotest 판독은 리포트 파일이 정본이며, autotest는 terminal-f 팬 **밖에서** 실행한다.

---

## §B Given-When-Then 시나리오

### 시나리오 1 — 신규 설치 사용자의 첫 실행

- **Given**: 저장된 UI 기본 설정이 없는(또는 `copyOnSelect`·`pwshIntegrationAuto` 키가 부재한) 환경에서 pwsh가 설치되어 있고,
- **When**: 앱을 처음 부팅하면,
- **Then**: (a) copy-on-select가 활성 상태이고, (b) 부팅 경로를 막지 않는 비동기 작업이 `multiline`·`cwd` 두 `$PROFILE` 블록을 설치하며, (c) 성공 후 상태표시줄에 새 PowerShell 팬 안내가 표시되고, (d) 다음 부팅에서는 재설치가 일어나지 않는다(스탬프 기록).

### 시나리오 2 — 기존 사용자의 명시적 비활성 존중

- **Given**: 저장된 UI 기본 설정에 `copyOnSelect: false`가 있고,
- **When**: 앱을 부팅하면,
- **Then**: copy-on-select는 비활성을 유지한다(키 부재 = 활성, 명시적 false 우선).

### 시나리오 3 — pwsh 미발견 환경의 재시도

- **Given**: pwsh·powershell 어느 쪽도 찾을 수 없는 환경이고,
- **When**: 첫 실행 자동 설치가 실패하면,
- **Then**: 버전 스탬프는 기록되지 않고, 다음 부팅에서 자동 설치가 다시 시도된다.

### 시나리오 4 — autotest 모드에서 프로필 무결성

- **Given**: `TERMF_AUTOTEST`가 설정된 autotest 실행이고,
- **When**: 부팅이 완료되어도,
- **Then**: 셸 통합 자동 설치는 실행되지 않아 개발 머신의 `$PROFILE`이 수정되지 않는다.

### 시나리오 5 — 수동 삭제한 사용자의 1회 재설치와 이후 존중

- **Given**: 이전 버전에서 설치 후 프로필 블록을 수동 삭제한 사용자(스탬프 부재)가 신버전을 처음 실행하면,
- **When**: 첫 실행 자동 설치가 블록을 다시 설치하고 스탬프를 기록하며,
- **Then**: 이후 같은 버전에서 사용자가 블록을 다시 삭제해도 앱은 재설치하지 않는다.

### 시나리오 6 — Part B 적용 후의 커맨드 팔레트

- **Given**: Part B가 적용된 빌드이고,
- **When**: 커맨드 팔레트를 열면,
- **Then**: `Shell: Enable multiline…`·`Shell: Enable live directory tracking…` 커맨드는 목록에 없고, Copy·Links 선호 토글은 그대로 존재하며, 안내 문서에는 죽은 메뉴를 가리키는 문구가 없다.

---

## §C 엣지 케이스 검증 항목

| 케이스 | 검증 방법 |
|---|---|
| 저장된 `copyOnSelect: false` 존중 | AC-1 구조 판정(`!== false` 패턴 자체가 false 우선을 보장) + 시나리오 2 수동 확인(§D.3) |
| 이전 버전 사용자의 1회 재설치(§A.4(a)) | 시나리오 5 수동 확인(§D.3) — 스탬프 부재/존재 조건은 코드 구조(AC-3/AC-5)로 고정 |
| pwsh 미발견 → 재시도(§A.4(b)) | AC-5 구조 판정(실패 경로 조기 반환, 스탬프 미기록) |
| pwsh 나중 설치 | AC-5와 동일 경로 — 다음 부팅 재시도로 자동 완료 |
| autotest 모드 프로필 무결성 | AC-4 구조 판정(`!bootInfo.autotest` 게이트) |
| cmd.exe 팬 | 기존 미지원 — 검증 대상 아님(spec §E) |
| 스니펫 변경 업그레이드(R8) | 절차 규정 — 본 SPEC 실행 시점에는 스니펫 변경이 없으므로 판정 대상 아님(`SHELL_INTG_AUTO_VER = "v1"` 불변 확인만) |
| Copy 토글 제목 표시 잔존 불일치(spec §D) | 미수리 기록 — 동작 무해, 검증 대상 아님 |

---

## §D AC 매트릭스

각 AC는 관찰 가능해야 한다(grep 결과, 테스트 출력, 리포트 파일). 특히 **AC-1~AC-5·AC-9·AC-14은 이번 도입으로 새로 생긴 가드가 아니라 이미 구현된 계약을 고정하는 트립와이어**다 — Part B가 같은 파일(`src/main.ts`)을 편집할 때 회귀를 잡는다.

| AC | 요구사항 | 시나리오 | 기준 | 검증 명령/방법 | 심각도 |
|---|---|---|---|---|---|
| AC-1 | R1 | 1, 2 | copy-on-select 기본 on 구조 — 부팅 읽기가 "명시적 false만 꺼짐" 패턴이고 모듈 초기값이 `true`다 | `grep -n "copyOnSelect !== false" src/main.ts` → 2건(`:878`, `:885`) · `grep -n "let copyOnSelect = true" src/terms.ts` → 1건(`:53`) | MUST |
| AC-2 | R1 | 2 | 저장된 `false` 우선 — `!== false` 패턴이 키 부재(→ true)와 명시적 false(→ false)를 구분하는 구조를 코드로 보장한다. 런타임 거동은 §D.3 수동 항목 | AC-1과 동일 grep의 패턴 해석(부재/true/false 3분기) | MUST |
| AC-3 | R3, R4 | 1 | 자동 설치 트리거·스탬프·비동기 구조 — 버전 스탬프 상수, autotest 게이트가 포함된 조건, 논블로킹 호출이 존재한다 | `grep -n "SHELL_INTG_AUTO_VER\|pwshIntegrationAuto\|autoInstallShellIntegration" src/main.ts` → 상수(`:166`), 함수(`:168`), 스탬프 기록(`:179`), 스냅샷 복원(`:880-881`), 트리거 `if (!bootInfo.autotest && uiPrefs.pwshIntegrationAuto !== SHELL_INTG_AUTO_VER)` + `void autoInstallShellIntegration()`(`:896-897`) 관측 · `grep -n "pwshIntegrationAuto" src/types.ts` → `:71` | MUST |
| AC-4 | R2 | 4 | autotest 모드 스킵 — 트리거 조건에 `!bootInfo.autotest` 게이트가 있다 | AC-3 grep의 트리거 조건문에서 게이트 관측 | MUST |
| AC-5 | R4, R6 | 3 | 스탬프는 성공 경로에서만 — 두 feature 설치 루프가 실패 시 조기 반환해 스탬프 기록에 도달하지 않는 구조다 | `grep -n -B6 "pwshIntegrationAuto = SHELL_INTG_AUTO_VER" src/main.ts` → 대입(`:179`)이 루프 성공 후 도달하며 실패 분기(`catch` → `return`, `:172-177`)가 그 앞에 있음을 관측 | MUST |
| AC-6 | R9 | 6 | 팔레트 설치 커맨드 부재 | `grep -c "shell.pwshMultiline\|shell.pwshCwd" src/main.ts` → **0** (M1 전 baseline: 2건 `:631`/`:641`) | MUST |
| AC-7 | R10 | 6 | 선호 토글 잔존 | `grep -c "copy.onSelect" src/main.ts` → 1 · `grep -c "links.toggleOpen" src/main.ts` → 1 | MUST |
| AC-8 | R11 | 6 | 죽은 코드 부재 또는 명시된 유지 사유 | `grep -n "installShellIntegration" src/main.ts` → 0건 **또는** progress.md §E.2에 기록된 유지 사유 · `grep -n "pwshIntegrationStatus" src/ipc.ts src/main.ts` → 0건 **또는** 동일한 유지 사유 기록 | MUST |
| AC-9 | R12 | — | 백엔드 설치 경로 보존 — 자동 설치가 의존하는 커맨드·구현이 그대로 있다 | `grep -n "install_pwsh_integration" src-tauri/src/commands.rs` → 존재 · `ls src-tauri/src/shellint.rs` → 존재 · `grep -n "installPwshIntegration" src/main.ts` → 자동 설치 호출 존재(`:171`) | MUST |
| AC-10 | R14 | — | Rust 스위트 무회귀 | `cd src-tauri && cargo test` → **142 + 1 + 5 passed, 0 failed 유지** — `pwsh_integration_status`를 커버하는 테스트가 없음을 plan-phase에서 확인했으므로 keep/remove 어느 쪽이든 수치 불변(수치가 달라지면 제거된 테스트명을 전량 열거할 것) | MUST |
| AC-11 | R14 | — | 프론트 타입 검사·번들 무회귀 | `npm run build` → `tsc --noEmit` exit 0 + vite 번들 성공 | MUST |
| AC-12 | R14 | — | autotest 무회귀 | 팬 밖 `TERMF_AUTOTEST=1` 실행 → `src-tauri/autotest-report.json`에서 `ok: true` | MUST |
| AC-13 | R13 | 6 | 문서 정리 — 현재 상태 문서에 죽은 메뉴 잔여 없음 | `grep -rn "Shell: Enable" docs/GUIDE-command-palette.md docs/GUIDE-features-easy.md` → **0** · `grep -n "Shell: Enable" docs/DEVELOPMENT.md` → 해당 트러블슈팅 행 갱신/제거 확인(파일 diff) · `docs/ADR-011-shell-integration-live-cwd.md`·`docs/PLAN-UX-polish.md`는 판정 대상 제외(역사 기록) | MUST |
| AC-14 | R5 | 1 | 자동 설치 성공 안내 잔존 — 제거 완료 후 안내 문구가 자동 설치 경로에 **정확히 1건**만 남는다(죽은 대화형 흐름의 동일 문구 3건이 Part B 제거로 사라졌음을 함께 증명) | `grep -c "Open a NEW PowerShell pane" src/main.ts` → **1** (M1 전 baseline **4건**: `:129`/`:147`/`:155`는 `installShellIntegration()` 내부 — 제거 대상, `:181`은 자동 설치 안내 — 잔존 대상) | MUST |

---

### §D.1 품질 게이트

| 게이트 | 기준 |
|---|---|
| Tested | 기존 Rust 스위트 green + 신규 실패 0(제거 작업 — 신규 커버리지 대상 없음. `shellint.rs` 기존 7테스트가 설치 로직 보존을 담당). TS는 구조 grep + autotest 두 축(프로젝트 제약상 러너 부재) |
| Readable | 코드 주석 한국어(`code_comments: ko`). 제거 후 주석 잔여(죽은 흐름을 설명하는 주석) 없음 |
| Unified | 기존 TS/Rust 스타일 준수. 신규 상수·의존성 없음 |
| Secured | 외부 입력 표면 변화 없음(제거만). `$PROFILE` 자동 기록은 이미 구현된 Part A 범위 |
| Trackable | Conventional Commits (`feat(SPEC-DEFAULTON-001): M1 ...`), 마일스톤 커밋 SHA를 progress.md에 기록 |

### §D.2 Definition of Done

- [ ] AC-1 ~ AC-14 전부 PASS (MUST)
- [ ] plan.md §A.1의 keep/remove 결정이 progress.md §E.2에 사유와 함께 기록됨
- [ ] `cargo test` + `npm run build` + autotest `ok: true` **셋 전부 확인** — tech.md §3 완료 선언 규칙(확인 전 완료 선언 금지, 미검증은 "미검증"으로 보고)
- [ ] Part A 계약 회귀 가드(AC-1~AC-5, AC-9)가 Part B 편집 후에도 성립 — 같은 파일 편집에 대한 회귀 확인
- [ ] plan.md §D PRESERVE 목록 무변경 확인(`git diff --stat`으로 스코프 검증)
- [ ] §F.S sync 산출물(CHANGELOG `[Unreleased]` 엔트리, frontmatter `completed` 전이) 완료

### §D.3 잔여 위험 (Residual Risk)

- **TS 배선·팔레트 상태에 대한 테스트 러너 부재(구조적 갭)**: 팔레트 목록 구성·부팅 읽기는 TS 영역인데 이 프로젝트에 TS 러너가 없다(신규 npm 의존성 금지 제약 — SPEC-PTY-FLOW-002 §D.3과 동일). 기계 가드는 (i) 구조 grep(AC-1~AC-8)과 (ii) 실기기 autotest(AC-12) 두 축이며, 둘 다 컴파일 시점 가드가 아니다. TS 러너 도입은 후속 SPEC 후보.
- **자동 설치 경로의 실기기 자동 검증 불가**: R2가 autotest에서 자동 설치를 스킵하므로(테스트 무결성을 위한 의도), 첫 실행 자동 설치·안내 표시는 autotest로 검증되지 않는다. **수동 검증 항목**: (1) 설정 초기화 후 첫 실행(시나리오 1), (2) `copyOnSelect: false` 저장 후 부팅(시나리오 2), (3) 이전 버전 프로필 상태에서 업그레이드 첫 실행(시나리오 5). M1 완료 보고 시 결과 또는 미실시 사유를 명시한다.
- **Copy 토글 제목 표시 잔존 불일치**: `copy.onSelect`의 플립(`main.ts:84`)·제목(`:622`)이 구형 `=== true` 관례를 유지해 키 부재 상태에서 "Enable"로 표시된다. 동작 무해(저장된 false 존중). 본 SPEC에서 고치지 않음(spec §E) — 후속 후보.
- **(a) 1회 재설치의 사용자 혼동**: 가시 안내로 완화했으나 실사용 반응은 미관측. 후속 조정 여부는 실사용 피드백에 의존.
- **pwsh 콜드 스타트로 안내 지연**: 첫 실행 안내가 부팅 직후 1초+ 뒤에 표시될 수 있다 — 비동기 설계의 의도된 비용(첫 화면 보호 우선).
