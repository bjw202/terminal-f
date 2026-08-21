---
id: SPEC-DEFAULTON-001
title: "기본 활성 UX 통합 — copy-on-select 기본 on·pwsh 셸 통합 첫 실행 자동 설치 + 커맨드 팔레트 설치 커맨드 정리"
version: "0.1.0"
status: in-progress
created: 2026-08-21
updated: 2026-08-21
author: manager-spec
priority: P2
phase: "v0.1.3 target"
module: "src, src-tauri/src, docs"
lifecycle: spec-anchored
tags: "ux, default-on, copy-on-select, shell-integration, pwsh, command-palette, first-launch"
tier: M
---

# SPEC-DEFAULTON-001 — 기본 활성 UX 통합 + 팔레트 설치 커맨드 정리

## HISTORY

| 버전 | 날짜 | 작성자 | 변경 내용 |
|---|---|---|---|
| 0.1.0 | 2026-08-21 | manager-spec | 최초 작성 (plan-phase 아티팩트 생성). **소급 기록 SPEC** — Part A(기본 활성 UX: copy-on-select 기본 on, pwsh 셸 통합 첫 실행 일회성 자동 설치)는 본 세션에서 이미 구현되어 작업 트리에 미커밋 상태로 존재하며 구현 세션에서 검증 완료. Part B(팔레트 수동 설치 커맨드 2건 제거 + 죽은 코드·문서 정리)가 유일한 run-phase 마일스톤이다. lifecycle `status`는 Part B run이 시작되기 전까지 `draft`를 유지한다 |

---

## §A 배경과 목적

### A.1 배경 — 옵트인에서 기본 활성으로

terminal-f의 대표 편의 기능 3가지 — 선택 즉시 복사(copy-on-select), Ctrl+click URL 열기, pwsh 셸 통합(멀티라인 입력 + 분할 창 실시간 디렉터리 추적) — 은 지금까지 옵트인이었다. 첫 실행 경험을 개선하기 위해 이들을 기본 활성으로 전환한다. 링크 열기(`openUrlOnClick`)는 이미 "명시적 false만 꺼짐" 기본 on 패턴으로 동작 중이므로 이번에 무변경이다.

### A.2 Part A — 이미 구현된 내용 (소급 기록, 2026-08-21 세션)

작업 트리 변경 파일 6건(미커밋, `main` 브랜치): `src/main.ts`, `src/terms.ts`, `src/types.ts`, `src-tauri/src/commands.rs`(doc 주석만), `docs/GUIDE-features-easy.md`, `docs/GUIDE-command-palette.md`.

**(1) copy-on-select 기본 on**

- 부팅 읽기가 `snap.ui?.copyOnSelect === true`에서 `snap.ui?.copyOnSelect !== false`로 변경(`src/main.ts:878`, 적용 `:885`) — `openUrlOnClick`과 동일한 "키 부재 = 활성, 명시적 false만 꺼짐" 패턴.
- 모듈 초기값 `let copyOnSelect = true`(`src/terms.ts:53`), `src/types.ts:64` 주석 갱신.
- 기존 사용자가 저장해 둔 `false`는 그대로 존중된다(명시적 비활성 승리).

**(2) 링크 열기(Ctrl+click)** — 이미 기본 on. 변경 없음, 작업 없음.

**(3) pwsh 셸 통합 첫 실행 일회성 자동 설치**

- 버전 스탬프: `UiPrefs.pwshIntegrationAuto?: string`(`src/types.ts:71`), 값 `"v1"`은 상수 `SHELL_INTG_AUTO_VER`(`src/main.ts:166`)로 관리.
- `autoInstallShellIntegration()`(`src/main.ts:168-182`): `["multiline","cwd"]`를 순회하며 기존 백엔드 커맨드 `install_pwsh_integration`을 호출(멱등 — 이전 버전 펜스 블록은 최신 스니펫으로 in-place 갱신, `src-tauri/src/shellint.rs`). 두 설치가 모두 성공한 뒤에만 스탬프를 기록(`:179`)·저장(`:180`)하고 상태표시줄에 일회성 안내를 표시한다(`:181`, "PowerShell multiline + directory tracking enabled. Open a NEW PowerShell pane to use it."). 실패 시 warn 후 조기 반환하여 스탬프를 남기지 않는다(`:172-177`).
- 트리거(`src/main.ts:893-898`): `boot()`에서 `void refreshTemplateCommands()`(`:891`) 이후, `!bootInfo.autotest && uiPrefs.pwshIntegrationAuto !== SHELL_INTG_AUTO_VER`일 때만 `void autoInstallShellIntegration()`로 시작 — 비동기 논블로킹이라 pwsh 콜드 스타트(1초+)가 첫 화면을 막지 않는다. `TERMF_AUTOTEST` 하에서는 스킵되어 테스트 실행이 개발 머신의 `$PROFILE`을 건드리지 않는다.
- 설계 결정: 셸 통합 스니펫이 변경되면 `SHELL_INTG_AUTO_VER`를 상향해 일회성 자동 설치를 재실행한다(기존 수동 "Update" 메뉴 흐름을 대체하는 갱신 경로).
- `src-tauri/src/commands.rs`의 `install_pwsh_integration` doc 주석 수정: 대화형 설치는 여전히 명시적 확인을 요구하며, 첫 실행 자동 설치가 유일하게 문서화된 예외임을 명시.
- 문서 갱신: 기본 활성 전환·첫 실행 자동 설치 서술(`docs/GUIDE-features-easy.md` §5, `docs/GUIDE-command-palette.md` §4-5 메뉴 사전 표 포함).

**검증(구현 세션 관측, 2026-08-21)**: `npm run build`(tsc --noEmit + vite, 39 modules) green, `cargo test`(142 + 1 + 5 passed, 0 failed) green. Part A·Part B가 같은 트리에 있으므로 이 결과는 M1 완료 시 전체 트리 대상 표준 스위트로 재확인된다(acceptance.md §D.2).

### A.3 Part B — 팔레트 설치 커맨드 정리 (run-phase 구현 대상)

자동 설치 도입으로 팔레트의 수동 설치 커맨드 2건(`shell.pwshMultiline`, `shell.pwshCwd`)의 역할이 사라진다: 신규 설치는 첫 실행 자동 설치가, 스니펫 갱신은 버전 스탬프 재실행이, pwsh 나중 설치는 실패 재시도 경로가 각각 덮는다. 이 커맨드들을 팔레트에서 제거하고, 그로부터 도달 불가능해지는 대화형 설치 흐름을 정리한다. **Copy·Links 토글은 설치가 아니라 선호 스위치이므로 유지한다.**

### A.4 수용된 알려진 엣지 (사용자 승인, 기록)

- **(a) 1회 재설치**: 이전 버전에서 설치 후 프로필 블록을 수동 삭제한 사용자는 신버전 첫 실행에 스탬프 부재로 블록이 1회 다시 설치된다(가시 안내 표시). 이후의 수동 삭제는 스탬프가 유효한 동안 존중된다(R7).
- **(b) pwsh 미발견**: 스탬프를 남기지 않아 매 부팅 저렴하게 재시도된다.
- **(c) cmd.exe 미지원**: 기존 제약 그대로(pwsh → powershell 폴백만 지원).

### A.5 채택된 방향 (사용자 승인 — 옵션 비교 확정)

pwsh 셸 통합의 기본 활성화 방식으로 **(A, 채택)** 첫 실행 일회성 `$PROFILE` 자동 설치를 선택했다. **(B, 기각)** 첫 실행 동의 대화상자(Install 사전 선택) — 문자 그대로의 default-on이 아니고 클릭 1회가 추가된다. **(C, 기각)** 사용자 파일을 건드리지 않는 pwsh 명령줄 per-session 주입 — OSC/프롬프트 래퍼 스니펫의 인용 부호·`$([char]27)` 처리로 argv 인용 위험이 있고 Rust spawn 경로 변경 폭이 크다. 전체 비교표는 plan.md §A.4.

---

## §B 요구사항 (GEARS)

GEARS 구조 키워드(`Where` / `While` / `When` / `shall`)와 코드 식별자는 영문 정본을 유지하고, 서술부는 한국어로 기술한다. **R1~R8은 Part A가 확립한 동작 계약(소급 기록 — 이미 구현됨), R9~R14는 Part B 요구사항(구현 대상)이다.**

### 명명 상수 · 식별자 (신규 임계값 없음)

| 식별자 | 값/의미 | 출처 |
|---|---|---|
| `SHELL_INTG_AUTO_VER` | `"v1"` — 셸 통합 자동 설치 버전 스탬프 상수 | `src/main.ts:166` |
| `UiPrefs.pwshIntegrationAuto` | 선택적 문자열 스탬프(마지막 자동 설치 버전) | `src/types.ts:71` |
| `TERMF_AUTOTEST` | autotest 모드 환경변수(설정 시 자동 설치 스킵) | 기존 |
| `shell.pwshMultiline` / `shell.pwshCwd` | 팔레트 수동 설치 커맨드 ID(Part B 제거 대상) | `src/main.ts:631` / `:641` |
| `copy.onSelect` / `links.toggleOpen` | 팔레트 선호 토글 ID(Part B 유지 대상) | `src/main.ts:621` / `:626` |

### R1 — copy-on-select 기본 활성 (Ubiquitous)

> The app **shall** 저장된 UI 기본 설정에 `copyOnSelect`가 명시적으로 `false`로 존재하지 않는 한 선택 즉시 복사를 활성 상태로 취급한다(키 부재 = 활성, 저장된 `false` 우선).

### R2 — autotest 모드에서 자동 설치 스킵 (State-driven)

> **While** autotest 모드(`TERMF_AUTOTEST`)가 켜져 있으면, the boot sequence **shall** 셸 통합 자동 설치를 전혀 실행하지 않는다.

### R3 — 첫 실행 자동 설치 트리거 (Event-driven)

> **When** autotest 모드가 아닌 부팅에서 버전 스탬프(`pwshIntegrationAuto`)가 `SHELL_INTG_AUTO_VER`와 일치하지 않으면, the app **shall** 자동 설치를 부팅 경로를 블로킹하지 않는 비동기 작업으로 시작한다.

### R4 — 자동 설치 내용과 스탬프 조건 (Ubiquitous)

> The auto-installer **shall** `multiline`(Ctrl/Shift+Enter 줄바꿈)과 `cwd`(OSC 9;9 분할 창 실시간 디렉터리 추적) 두 블록을 멱등 백엔드 설치 커맨드(`install_pwsh_integration`)로 설치하고, 두 설치가 모두 성공한 뒤에만 버전 스탬프를 갱신한다.

### R5 — 성공 안내 (Event-driven)

> **When** 자동 설치가 성공하면, the status bar **shall** 새 PowerShell 팬을 열라는 일회성 안내를 표시한다.

### R6 — 실패 시 다음 부팅 재시도 (Event-driven)

> **When** 자동 설치 시도가 실패하면(pwsh 미발견 포함), the app **shall** 버전 스탬프를 기록하지 않은 상태로 두어 다음 부팅에서 자동으로 재시도한다.

### R7 — 스탬프 유효 중 수동 삭제 존중 (Unwanted)

> The app **shall not** 버전 스탬프가 `SHELL_INTG_AUTO_VER`와 일치하는 동안 사용자가 임의로 삭제한 `$PROFILE` 셸 통합 블록을 재설치하지 않는다.

### R8 — 스니펫 변경 시 재실행 (Event-driven)

> **When** 셸 통합 스니펫 변경으로 `SHELL_INTG_AUTO_VER`가 상향되면, the app **shall** 다음 부팅에서 자동 설치를 다시 실행하여 기존 펜스 블록을 최신 스니펫으로 갱신한다.

### R9 — 팔레트 수동 설치 커맨드 제거 (Unwanted)

> The command palette **shall not** `shell.pwshMultiline` 및 `shell.pwshCwd` 커맨드를 목록에 노출하지 않는다.

### R10 — 선호 토글 유지 (Ubiquitous)

> The command palette **shall** Copy(`copy.onSelect`) 및 Links(`links.toggleOpen`) 선호 토글을 계속 노출하고 그 동작을 변경하지 않는다.

### R11 — 죽은 코드 미잔존 (Ubiquitous)

> The frontend **shall** 제거된 팔레트 커맨드로부터 도달 불가능해진 대화형 설치 흐름(`installShellIntegration()`)과 그 전용 IPC 래퍼(`ipc.pwshIntegrationStatus`)를 죽은 코드로 남기지 않는다 — 제거하거나, 유지 시 그 사유를 plan.md에 명시한다.

### R12 — 백엔드 설치 경로 보존 (Ubiquitous)

> The backend **shall** `install_pwsh_integration` 커맨드와 셸 통합 설치 구현(`shellint.rs`, 펜스 블록 in-place 갱신 포함)을 변경 없이 유지한다 — 자동 설치가 의존하는 경로다.

### R13 — 문서 동기화 (Ubiquitous)

> The user guide **shall** 기본 활성 전환과 첫 실행 자동 설치를 반영하고, 제거된 팔레트 설치 커맨드·수동 설치 안내의 잔여 문구를 현재 상태 문서(`GUIDE-command-palette.md`, `GUIDE-features-easy.md`, `DEVELOPMENT.md`)에 남기지 않는다. 과거 기록(ADR, PLAN)은 역사 기록으로 보존한다.

### R14 — 기존 동작 무회귀 (Ubiquitous)

> The app **shall** 기존 Rust 테스트 스위트와 프론트엔드 타입 검사·번들, autotest 체크를 전부 통과한다.

---

## §C 제약 (Constraints)

| 구분 | 제약 |
|---|---|
| 개발 방법론 | TDD (RED → GREEN → REFACTOR), `quality.yaml constitution.development_mode: tdd`. 단 Part B는 제거 중심 작업이라 "새 실패 테스트"가 대상 자체를 형성하지 않는다 — RED 역할은 **제거 전 존재 grep**(현행 `src/main.ts:631/:641` 2건)가, GREEN은 **부재 grep**이 담당한다(plan.md §A.2). |
| 테스트 인프라 | TS 테스트 러너(vitest/jest 등) 도입 금지(신규 npm 의존성 금지와 동일 축 — SPEC-PTY-FLOW-002 §C와 동일 제약). TS 쪽 검증은 구조 grep + 실기기 autotest 두 축으로 한정한다. |
| autotest | autotest(`TERMF_AUTOTEST=1`)는 앱을 자체 종료시킨다 — **terminal-f 팬 안에서 실행 금지**, 리포트 파일이 정본. autotest 체크 로직·판정은 변경 금지. |
| 백엔드 무변경 | `install_pwsh_integration`, `shellint.rs` 전체(R12). `pwsh_integration_status` 커맨드의 제거 여부는 plan.md §A.1의 절차적 결정(자동 설치가 사용하지 않는 표면). |
| autotest 무결성 | `src/autotest.ts`의 어떤 체크도 깨뜨리지 않는다. IPC 표면을 건드리기 전에 반드시 사용처 grep을 재실행한다(plan-phase 관측: `pwshIntegrationStatus`의 autotest 사용 0건). |
| Part A 불가침 | Part A가 확립한 부팅 읽기·자동 설치·스탬프 구조는 재수정 금지(소급 기록 대상). Part B는 같은 파일(`src/main.ts`)을 공유하되 변경 단위는 팔레트 등록부·죽은 대화형 흐름에 한한다. |
| 완료 선언 규칙 | 프로젝트 규칙(`.moai/project/tech.md` §3): `cargo test` + `npm run build` + autotest `ok: true`를 **전부 확인하기 전에는 완료라고 말하지 않는다**. 미검증 항목은 "미검증"으로 보고한다. |
| 문서 언어 | 모든 문서는 한국어(코드 식별자·파일명·명령어는 영어 유지). |
| @MX 태그 | 자동 설치 트리거·스탬프 계약에 `@MX:ANCHOR` 후보(`code_comments: ko`). 제거 작업 자체에는 신규 태그 불필요. |

---

## §D 부록 — 엣지 케이스

| 케이스 | 기대 동작 |
|---|---|
| 키 부재(신규 설치 사용자) | copy-on-select 활성(R1) + 첫 실행 자동 설치 실행(R3) |
| 저장된 `copyOnSelect: false` | 비활성 유지 — 명시적 false가 우선(R1) |
| 이전 버전에서 수동 삭제한 사용자 | 신버전 첫 실행에 1회 재설치 + 가시 안내, 이후 수동 삭제 존중(§A.4(a), R7) |
| pwsh 미발견 환경 | 스탬프 미기록 → 매 부팅 재시도(§A.4(b), R6). 안내 없이 조용히 재시도 |
| pwsh를 나중에 설치한 환경 | 다음 부팅의 재시도 경로가 자동 설치를 완료(R6) |
| autotest 모드 부팅 | 자동 설치 스킵, 프로필 미수정(R2) — 개발 머신 보호 |
| cmd.exe 팬 | 셸 통합 미적용(기존 미지원, §A.4(c)) |
| 스니펫이 변경된 업그레이드 | `SHELL_INTG_AUTO_VER` 상향 → 다음 부팅에 펜스 블록 in-place 갱신(R8) |
| **Copy 토글 제목 표시 잔존 불일치(관측 기록)** | `copy.onSelect`의 플립(`src/main.ts:84`, `=== true` 기준)과 제목(`:622`, truthiness 기준)이 구형 default-off 관례를 유지해, 키 부재 상태에서 제목이 "Enable"로 표시된다. 동작은 무해하다(첫 클릭이 명시적 `true`를 기록할 뿐, 저장된 `false`는 존중됨). **본 SPEC에서 수정하지 않는다** — 화면 표시 일관성은 후속 후보(§E). |
| 링크 열기(Ctrl+click) | 무변경 — 이미 기본 on |

---

## §E 범위 제외 (Exclusions)

본 SPEC이 **의도적으로 만들지 않는** 것들이다. 미래의 독자가 "버그"로 오인하지 않도록 명시한다.

### Out of Scope — cmd.exe 셸 통합 지원

- cmd.exe는 여전히 지원하지 않는다(pwsh → powershell 폴백만). §A.4(c).
- cmd 통합 스니펫 설계는 요구가 확인되면 별도 SPEC이다.

### Out of Scope — Copy·Links 선호 토글 제거

- `copy.onSelect`·`links.toggleOpen`은 설치 커맨드가 아니라 선호 스위치다 — Part B가 제거하는 대상이 아니다(R10).

### Out of Scope — 셸 통합 스니펫 내용 변경

- `"v1"` 스니펫의 내용은 변경하지 않는다. 스니펫 수정은 `SHELL_INTG_AUTO_VER` 상향 절차와 함께 별도 변경으로 처리한다(R8은 절차만 규정).

### Out of Scope — 백엔드 설치 커맨드 재설계

- `install_pwsh_integration`·`shellint.rs`의 설계·프로토콜은 변경하지 않는다(R12). Part B의 백엔드 접점은 사용처가 사라진 `pwsh_integration_status`의 제거 여부뿐이다.

### Out of Scope — 링크 열기 동작 변경

- `openUrlOnClick`의 게이트·위험 스킴 거부 등 링크 열기 동작은 무변경(이미 기본 on).

### Out of Scope — Part A 재구현·재수정

- Part A는 이미 구현·검증되어 작업 트리에 있다. 본 SPEC은 그 기록이며, run-phase가 Part A 코드를 다시 손대는 것을 포함하지 않는다(§C Part A 불가침).

### Out of Scope — ADR·PLAN 과거 기록 재작성

- `docs/ADR-011-shell-integration-live-cwd.md:35`, `docs/PLAN-UX-polish.md:114/:174`의 "Shell: Enable" 문구는 당시 옵트인 설계의 역사 기록이다 — 현재 상태 문서가 아니므로 "정리" 대상에서 제외한다(R13).

### Out of Scope — Copy 토글 제목 표시 일관성

- §D의 `copy.onSelect` 제목/플립 잔존 불일치는 동작에 영향이 없어 본 SPEC에서 고치지 않는다. 후속 후보로 기록만 남긴다.

---

## §F 참조

- `.moai/specs/SPEC-DEFAULTON-001/plan.md` — 구현 계획(마일스톤 M1 + §F.S), 기술 결정, Part A 옵션 비교표
- `.moai/specs/SPEC-DEFAULTON-001/acceptance.md` — 수용 기준 AC-1~AC-14, Given-When-Then
- `.moai/specs/SPEC-DEFAULTON-001/progress.md` — §E.1 plan-phase 신호 + 검증 증거
- `src/main.ts`, `src/terms.ts`, `src/types.ts`, `src/ipc.ts` — Part A 구현 + Part B 변경 대상
- `src-tauri/src/commands.rs`, `src-tauri/src/shellint.rs`, `src-tauri/src/lib.rs` — 백엔드 설치 경로
- `docs/GUIDE-features-easy.md`, `docs/GUIDE-command-palette.md`, `docs/DEVELOPMENT.md` — Part B 문서 정리 대상
- `docs/ADR-011-shell-integration-live-cwd.md` — 셸 통합 원 설계(역사 기록)
- `.moai/project/tech.md` §3 — 검증 순서·완료 선언 규칙
