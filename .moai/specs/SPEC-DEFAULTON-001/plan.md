# SPEC-DEFAULTON-001 — 구현 계획 (plan)

- SPEC: `.moai/specs/SPEC-DEFAULTON-001/spec.md` (GEARS R1~R14)
- Tier: M · cycle_type: **tdd** (프로젝트 기본 — 제거 중심 작업에 대한 RED 대체 규정은 §A.2) · Route: A (Hybrid Trunk main-direct)
- depends_on: 없음
- 특이사항: **소급 기록 SPEC** — Part A는 이미 구현·검증 완료(작업 트리 미커밋 6파일, spec.md §A.2). run-phase는 Part B 단일 마일스톤 M1뿐이다
- 커밋 스포범 — **Part A 선행 커밋 6파일**(이미 구현·미커밋, §F M1 step 0이 커밋 배정): `src/main.ts`, `src/terms.ts`, `src/types.ts`, `src-tauri/src/commands.rs`, `docs/GUIDE-features-easy.md`, `docs/GUIDE-command-palette.md`
- 추정 변경 파일(Part B M1): `src/main.ts`, `src/ipc.ts`(조건부), `src-tauri/src/commands.rs`(조건부), `src-tauri/src/lib.rs`(조건부), `docs/GUIDE-command-palette.md`, `docs/GUIDE-features-easy.md`, `docs/DEVELOPMENT.md`

---

## §A 기술 결정 (가역성 높은 순 — 바꿀 가능성이 큰 결정부터)

### A.1 `pwsh_integration_status` IPC 표면의 제거 여부 (가장 되돌리기 어려운 결정 — API 표면)

**plan-phase 관측(2026-08-21, grep)**: 프론트 래퍼 `ipc.pwshIntegrationStatus`(`src/ipc.ts:117-118`)의 호출자는 `src/main.ts:114`(`installShellIntegration()` 내부) **1곳**뿐이고 `src/autotest.ts` 사용은 **0건**이다. 백엔드 `pwsh_integration_status`(`src-tauri/src/commands.rs:1251`, 등록 `src-tauri/src/lib.rs:175`)에도 다른 호출 경로가 없다.

**기본 방향**: Part B로 `installShellIntegration()`가 죽으면 그 전용 래퍼도 함께 제거한다 — 제거 표면은 (i) `src/ipc.ts` 래퍼 + 관련 타입(`PwshIntegrationInfo`가 다른 곳에서 쓰이지 않을 때), (ii) `src-tauri/src/commands.rs` 커맨드, (iii) `src-tauri/src/lib.rs` 등록 3곳이다. 유지를 선택할 경우 그 사유(예: 향후 "설치 상태 표시" 기능 복귀 계획)를 반드시 progress.md §E.2에 기록한다.

**run-phase 의무**: 제거 확정 전 §C 사전 점검의 사용처 grep을 **재실행**한다(특히 `src/autotest.ts` — plan-phase 스냅샷과 달라질 수 있다). 재확인 없이 제거하는 것은 §G 금지 사항이다.

### A.2 팔레트 설치 커맨드 제거 + 대화형 흐름 정리

- 제거 대상 등록: `shell.pwshMultiline`(`src/main.ts:630-639`), `shell.pwshCwd`(`:640-649`).
- 두 등록의 `run:`이 `installShellIntegration()`의 유일한 호출부(`:634`, `:644`)다 — 등록 제거와 함께 `installShellIntegration()`(`:104-160`) 본체와 그 전용 보조 흐름(상태 안내 `showStatus("Checking PowerShell profile…")`, `confirmModal`·`listModal` 대화 상자 사용처)이 죽는다. 본체를 제거한다(R11). 단 `confirmModal`/`listModal`/`showStatus` 함수 자체는 다른 호출자가 있으므로 존재를 유지한다 — 제거하는 것은 **이 흐름의 사용처**뿐이다(run-phase에서 호출자 수 재확인).
- `copy.onSelect`(`:620-624`), `links.toggleOpen`(`:625-629`)은 무변경(R10).
- **TDD RED 대체 규정(§C 개발 방법론의 근거)**: 팔레트는 TS 영역이고 이 프로젝트에 TS 테스트 러너가 없어 "실패하는 테스트"를 작성할 수 없다. 따라서 RED 상당 증거는 **제거 전 존재 grep verbatim**(`shell.pwshMultiline` / `shell.pwshCwd` 각 1건, `installShellIntegration` 4매치)이고, GREEN은 AC-6/AC-8의 **부재 grep(0건)** 이다. 이는 SPEC-PTY-FLOW-002 §D.3이 확립한 "구조 grep + 실기기 autotest 두 축" 회귀 가드와 같은 축이다.

### A.3 문서 정리 범위 (grep 정밀도 — 판정 대상 명시)

plan-phase grep `"Shell: Enable"` 결과와 처치:

| 위치 | 성격 | 처치 |
|---|---|---|
| `docs/GUIDE-command-palette.md:176/:188` | 현재 상태 메뉴 문서의 커맨드 행 2건 | **제거** (AC-13 판정 대상) |
| `docs/GUIDE-features-easy.md:196/:207` | "직접 설치하려면…" 수동 설치 산문 | **제거·재서술** (자동 설치 안내로) |
| `docs/GUIDE-features-easy.md:263/:264` | 메뉴 사전 표 2행 | **제거·수정** |
| `docs/DEVELOPMENT.md:187` | 메뉴 클릭 지연 트러블슈팅 행(`pwsh_integration_status` 콜드 스타트 설명) | **갱신 필요** — 메뉴 흐름 소멸로 서술 근거가 사라짐. A.1에서 백엔드 커맨드를 제거했다면 행 자체 제거, 유지했다면 자동 설치 맥락으로 재서술 |
| `docs/ADR-011:35`, `docs/PLAN-UX-polish.md:114/:174` | 과거 기록 | **보존** — 판정 대상 아님(spec §E) |

AC-13의 grep은 GUIDE 2종만 대상으로 0을 기대한다. `grep -rn "Shell: Enable" docs/` 전체 0은 ADR까지 지우라는 잘못된 기준이 되므로 채택하지 않는다(사용자 제안 기준의 정밀도 보정).

### A.4 Part A 설계 기록 (소급 — 변경 대상 아님)

**구현 구조**(spec.md §A.2에 전체 기록): 부팅 읽기 `!== false` 패턴(`main.ts:878/:885`), 모듈 초기값(`terms.ts:53`), 버전 스탬프(`types.ts:71` + `main.ts:166`), 자동 설치(`main.ts:168-182` — 실패 시 조기 반환으로 스탬프 미기록), 트리거(`main.ts:893-898` — `refreshTemplateCommands()` 이후, `!bootInfo.autotest` 게이트, `void` 비동기), commands.rs doc 주석, GUIDE 2종.

**자동 설치 방식 옵션 비교(사용자 승인 — A 채택)**:

| 옵션 | 방식 | 판정 | 사유 |
|---|---|---|---|
| **A (채택)** | 첫 실행 일회성 `$PROFILE` 자동 설치(스탬프로 1회 보장) | 채택 | 문자 그대로 default-on. 멱등 설치 커맨드 재사용으로 백엔드 변경 최소. 실패 재시도·버전 상향 재실행이 자연히 성립 |
| B | 첫 실행 동의 대화상자(Install 사전 선택) | 기각 | default-on이 아님(클릭 1회 추가). 대화 상자 코드 경로 유지 비용 |
| C | pwsh 명령줄 per-session 주입(사용자 파일 무수정) | 기각 | OSC/프롬프트 래퍼 스니펫의 인용 부호·`$([char]27)`로 argv 인용 위험. Rust spawn 경로 변경 폭이 크고 새 팬마다 주입 필요 |

**버전 스탬프 재실행 설계(R8)**: 스니펫을 변경할 때 `SHELL_INTG_AUTO_VER`를 상향한다(`"v1"` → `"v2"` …). 다음 부팅에서 스탬프 불일치 → 자동 설치 재실행 → 백엔드의 펜스 블록 in-place 갱신(`shellint.rs`) → 스탬프 재기록. 이 경로가 기존 수동 "Update" 메뉴 흐름을 대체하며, 그래서 Part B에서 메뉴 제거가 안전하다.

### A.5 미해결 클래리피케이션 없음

NEEDS CLARIFICATION 마커 잔여 0건. Part A·Part B의 모든 범위·설계 결정은 사용자 승인 완료다(spec.md §A.4~§A.5, progress.md §E.1 조사 경위).

---

## §B 알려진 리스크 (Known Issues)

| # | 리스크 | 완화 |
|---|---|---|
| B1 | `src/autotest.ts`가 status IPC를 몰래 사용 중일 수 있음 | plan-phase grep으로 사용 0건 확인 완료. 그래도 제거 전 run-phase 재grep 의무(§A.1). autotest 체크 로직은 PRESERVE |
| B2 | 백엔드 status 커맨드 제거가 등록·타입 경로에 영향 | 영향 표면 열거 완료(commands.rs:1251 + lib.rs:175 + ipc.ts:117 + `PwshIntegrationInfo` 타입). 제거 직후 `cargo test`(AC-10)·`npm run build`(AC-11)로 즉시 검증 |
| B3 | `copy.onSelect` 플립·제목의 `=== true`/truthiness 잔존(키 부재 시 "Enable" 표시 — spec §D 기록) | 동작 무해(저장된 false 존중, 첫 클릭은 명시적 true 기록). 본 SPEC 스코프 밖 — 후속 후보로 등재만 |
| B4 | 옛 메뉴 경로에 익숙한 사용자의 혼란 | GUIDE에 자동 설치·버전 상향 재실행 서술로 대응(A.3). 별도 마이그레이션 UI 없음 |
| B5 | §A.4(a) 엣지 — 수동 삭제 사용자의 1회 재설치 | 상태표시줄 가시 안내로 이미 완화. GUIDE 문서에도 서술 |
| B6 | TS 러너 부재 — 팔레트 상태·부팅 동작의 자동 가드 없음 | 구조 grep(AC-1/3/4/6/7/8) + 실기기 autotest(AC-12) 두 축. SPEC-PTY-FLOW-002 §D.3과 동일한 구조적 갭 — acceptance §D.3 잔여 위험 등재 |
| B7 | 자동 설치 경로는 autotest에서 스킵(R2)되어 실기기 자동 검증 불가 | 테스트 무결성을 위한 의도된 스킵. 실기기 첫 실행(신규 설치 1회)은 수동 검증 항목으로 잔여 위험 등재 |
| B8 | autotest를 terminal-f 팬 안에서 실행하여 세션 자체 종료 | 팬 밖 실행. 리포트 파일(`src-tauri/autotest-report.json`)이 정본 |
| B9 | Part A와 Part B가 `src/main.ts`를 공유 — 스코프 번짐 | 스코프를 변경 단위로 규정: Part B는 팔레트 등록부·죽은 대화형 흐름에 한함. 부팅 읽기·자동 설치·스탬프 영역은 PRESERVE(§D) |
| B10 | `docs/DEVELOPMENT.md:187` 트러블슈팅 행을 놓쳐 죽은 메뉴를 안내하는 문서 잔존 | AC-13 판정 대상에 DEVELOPMENT.md 갱신을 명시 포함(A.3) |

---

## §C 사전 점검 (Pre-flight)

```bash
# 0. Part A 변경이 작업 트리에 있는지 확인 (6건 M 상태 기대)
git -C <root> status --short

# 1. 제거 대상 존재 확인 (RED 스냅샷 — verbatim 캡처)
grep -n "shell.pwshMultiline\|shell.pwshCwd" src/main.ts        # 2건 (:631, :641)
grep -n "installShellIntegration" src/main.ts                    # 정의 + 호출 2 + 주석
grep -n "pwshIntegrationStatus" src/ipc.ts src/main.ts           # 래퍼 정의 + 유일 호출자 (:114)

# 2. A.1 판단 재확인 (autotest 무관계 재검증 — 0건 기대)
grep -n "pwshIntegrationStatus\|installShellIntegration\|pwsh_integration_status" src/autotest.ts
grep -rn "pwsh_integration_status" src-tauri/src                 # commands.rs:1251 + lib.rs:175

# 3. baseline (기존 green 구분용)
cd src-tauri && cargo test 2>&1 | tail -5                        # 142 + 1 + 5 passed
cd .. && npm run build                                           # tsc + vite green
```

---

## §D 제약 (DO NOT VIOLATE)

PRESERVE 목록 — 다음은 **변경 금지**다.

- 백엔드 `install_pwsh_integration` 커맨드와 `src-tauri/src/shellint.rs` 전체(펜스 블록 in-place 갱신 포함) — R12
- `src/autotest.ts`의 체크 로직·판정·집계식 전체
- 팔레트 `copy.onSelect`·`links.toggleOpen` 등록·플립·동작 — R10
- Part A가 확립한 영역: 부팅 읽기 `copyOnSelect !== false`(`main.ts:878/:885`), 자동 설치(`:168-182`), 트리거(`:893-898`), 스탬프 구조, commands.rs doc 주석
- `openUrlOnClick` 링크 열기 동작 전체(게이트·위험 스킴 거부 포함)
- `docs/ADR-011-shell-integration-live-cwd.md`, `docs/PLAN-UX-polish.md`의 과거 기록 서술

금지 명령: `git push --no-verify`, 푸시 후 `git commit --amend`, `git push --force`, `git add -A`(명시 경로만 스테이징).
필수: Conventional Commits — (i) Part A 선행 커밋 `feat(SPEC-DEFAULTON-001): Part A 기본 활성 전환`(§F M1 step 0, 명시 pathspec 6파일. 이 커밋이 본 SPEC의 첫 run-phase 커밋이므로 `draft → in-progress` frontmatter 전이가 함께 탄다), (ii) `feat(SPEC-DEFAULTON-001): M1 <subject>`. 공통: `🗿 MoAI` 트레일러, 커밋 메시지 한국어(`git_commit_messages: ko`).

---

## §E 자기 검증 (Self-Verification)

각 마일스톤 완료 보고는 검증 주장 무결성 5절 형식(주장 / 증거 / baseline 귀속 / 미검증 / 잔여 위험)을 따른다. 최소 제출물:

| 항목 | 명령 | 기대 |
|---|---|---|
| E1 AC 매트릭스 | acceptance.md AC별 PASS/FAIL + 검증 명령 + verbatim 출력 | 전 MUST PASS |
| E2 RED 상당 증거 | 제거 **전** 존재 grep 출력 verbatim(§C 항목 1) | 존재 2건+ 확인 |
| E3 Rust 테스트 | `cd src-tauri && cargo test` | 142 + 1 + 5 passed, 신규 실패 0 |
| E4 프론트 빌드 | `npm run build` | tsc exit 0 + vite 번들 성공 |
| E5 잔존 grep | 팔레트 ID → 0 · GUIDE 2종 "Shell: Enable" → 0 · DEVELOPMENT.md 갱신 확인 | 전부 성립 |
| E6 autotest | 팬 밖 `TERMF_AUTOTEST=1` 실행 → 리포트 판독 | `ok: true` |
| E7 커밋/푸시 | `git log --oneline`, push 결과, push 후 `git status --short` | 마일스톤 커밋 SHA 명시(Part A 선행 커밋 + M1 커밋) · push 후 `git status --short`에서 src/·docs/의 본 SPEC 스코프에 잔여 ` M ` 엔트리 **0건**(Part A 6파일 미커밋 잔존 방지 — D1) |

---

## §F 마일스톤 (독립 검증 가능 단위)

단일 마일스톤 — Part B는 한 덩어리 cleanup이며 분리 가능한 하위 검증 단위가 없다(제거·정리·문서가 같은 스냅샷에서만 함께 판정된다).

### M1 — 팔레트 설치 커맨드 제거 + 죽은 코드·문서 정리 (spec R9~R14)

내부 순서(커밋 보장 → 결정 → 기계 → 문서 → 검증):

0. **Part A 선행 커밋 (D1)**: §C 사전 점검 먼저 실행(항목 0 — Part A 6파일 ` M ` 상태 확인, 사용처 재grep 포함). 확인 직후 그 6파일(`src/main.ts`, `src/terms.ts`, `src/types.ts`, `src-tauri/src/commands.rs`, `docs/GUIDE-features-easy.md`, `docs/GUIDE-command-palette.md`)을 **명시 pathspec으로만** 스테이징해 `feat(SPEC-DEFAULTON-001): Part A 기본 활성 전환` 커밋 — 스테이징 직전 `git status --short` 재확인으로 비스코프 파일(`.moai/reports/` 등) 배제. 이 커밋이 본 SPEC의 첫 run-phase 커밋이므로 `draft → in-progress` 전이가 함께 탄다. `terms.ts`(모듈 초기값)·`types.ts`(스탬프 필드)가 커밋 누락되면 배포 빌드는 copy-on-select 기본 off로 조용히 회귀하므로(M1 grep은 작업 트리에서만 green), 이 단계가 R1~R8의 착지 보장이다.
1. §A.1 keep/remove 확정(사전 점검 결과 기반).
2. 팔레트에서 `shell.pwshMultiline`·`shell.pwshCwd` 등록 제거. `installShellIntegration()` 본체와 그 전용 흐름(상태 안내·모달 사용처) 제거. `confirmModal`/`listModal`/`showStatus` 함수 자체는 유지(다른 호출자 존재 — 재확인).
3. (1)에서의 확정에 따라 `ipc.pwshIntegrationStatus` 래퍼 + 백엔드 `pwsh_integration_status` + lib.rs 등록 제거 — 또는 유지 사유를 progress.md §E.2에 기록.
4. 문서 정리(§A.3 표): GUIDE-command-palette.md 메뉴 행 2건, GUIDE-features-easy.md 산문 2곳 + 메뉴 사전 표 2행, DEVELOPMENT.md:187 행 갱신.
5. 검증·커밋: E1~E6 판정 후 E7 커밋·푸시(Part A 선행 커밋은 step 0에서 완료). AC-6~AC-14 판정 + AC-1~AC-5, AC-9 구조 재확인(Part A 계약 회귀 가드 — AC-14의 "정확히 1건"은 제거 완료 후에만 성립한다).
6. 선택: 자동 설치 트리거·스탬프 계약에 `@MX:ANCHOR` 부착(한국어).

### §F.S — sync 단계 산출물

- `CHANGELOG.md` `[Unreleased]` 엔트리 — Part A 기본 활성 전환(copy-on-select·셸 통합 자동 설치) + Part B 팔레트 정리를 아우르는 1건.
- `docs/DEVELOPMENT.md`·`GUIDE` 2종의 최종 확인(AC-13).
- (판단) `docs/ADR-011`에 기본 활성 전환을 반영한 상태 각주 추가 여부 — 역사 기록 재작성은 아니고 상태 주석만 허용.
- spec.md frontmatter `in-progress → implemented → completed` 전이 + progress.md §E.4.

---

## §G 안티패턴 (금지)

- **autotest 재grep 없이 IPC 표면 제거** — plan-phase 스냅샷은 만료될 수 있다(§A.1 절차 우회).
- **Copy/Links 토글까지 제거** — R10 위반. 설치 커맨드와 선호 스위치를 혼동한 것이다.
- **백엔드 `install_pwsh_integration` 제거** — R12 위반. 자동 설치가 의존하는 경로다.
- **Part A 부팅 읽기·자동 설치 코드 재수정** — 스코프 밖(§D PRESERVE).
- **ADR·PLAN의 "Shell: Enable" 문구를 정리 명목으로 재작성** — 역사 기록 보존 위반(spec §E).
- **`docs/DEVELOPMENT.md:187` 행을 놓치고 "문서 정리 완료" 보고** — 죽은 메뉴를 안내하는 문서 잔존(B10).
- **grep 없이 "완료" 선언** — tech.md §3 완료 선언 규칙 위반(cargo test + build + autotest 셋 전부).
- **autotest를 terminal-f 팬 안에서 실행** — 앱이 자체 종료된다(B8).
- **사유 없이 죽은 코드 남기기 / 사유 없이 백엔드 표면 제거** — R11·§A.1 절차 양쪽 다 우회하는 것이다.

---

## §H 참조

- `.moai/specs/SPEC-DEFAULTON-001/spec.md` — 요구사항 R1~R14, 범위 제외
- `.moai/specs/SPEC-DEFAULTON-001/acceptance.md` — AC-1~AC-14 (논리 14건 — plan-audit iteration 1 D2 반영으로 AC-14 신설)
- `.moai/specs/SPEC-DEFAULTON-001/progress.md` — §E.1 plan-phase 신호 + 검증 증거
- `.moai/project/tech.md` §3 — 검증 순서·완료 선언 규칙
- `docs/ADR-011-shell-integration-live-cwd.md` — 셸 통합 원 설계(역사 기록)
