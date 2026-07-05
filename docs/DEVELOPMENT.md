# terminal-f 개발 가이드 (세션 인수인계용)

> 이 문서는 **다음 개발 세션(사람 또는 AI)이 이 코드베이스를 이어받아
> 디버깅하거나 기능을 추가할 때** 필요한 모든 실무 지식을 담는다.
> 아키텍처의 "왜"는 ADR-001~010, 기능 로드맵은 PLAN-M1-M2-roadmap.md 참고.
> **새 기능을 완성하면 이 문서의 해당 절도 갱신할 것.**

---

## 1. 30초 요약

Windows 네이티브 터미널 에뮬레이터. **Tauri 2 + Rust 백엔드**(portable-pty
/ ConPTY)와 **TypeScript + xterm.js 프론트엔드**(프레임워크 없음, vanilla).

제1원칙: **백엔드가 유일한 진실이다.** PTY 세션, pane 트리, 워크스페이스,
자동화 규칙, 주입 게이트 전부 Rust가 소유하고, 프론트는 렌더링과 커맨드
호출만 한다. 프론트에서 레이아웃을 직접 바꾸는 코드를 추가하면 안 된다.

구현 완료 상태 (2026-07-05): M0 코어 + Phase A(UI) + M2.0(주입 게이트) +
M2.1(rule engine) + M2.1.5(timer) + M2.2(named pipe 컨트롤 API) +
Phase B(템플릿) + 이미지 붙여넣기/파일 드롭 브리지 +
UX①~④ 전부 완료: ①복사(스마트 Ctrl+C / Ctrl+Shift+C / 우클릭 / copy-on-select
/ OSC 52) + ②Ctrl+Enter 멀티라인(IME 순서 안전 + pwsh Alt+Enter opt-in) +
③라이브 cwd(OSC 9;9 셸 통합, ADR-011) + ④Campbell PowerShell 테마.
config schemaVersion **7**. Rust 테스트 89개(scan_cwd 6 / shellint 7 포함), autotest 29개 검사. ADR-001~011.

---

## 2. 모듈 지도

### 백엔드 `src-tauri/src/`

| 파일 | 역할 |
|---|---|
| `lib.rs` | 앱 부팅: config 로드 → AppState 구성 → emitter/automation/pipe 스레드 기동 → 커맨드 등록. **새 tauri 커맨드는 여기 `invoke_handler`에 반드시 등록** |
| `model.rs` | 직렬화 모델(Config, Workspace, PaneNode, PaneLeaf, SplitNode). `CONFIG_SCHEMA_VERSION` 상수. `now_ms()`, `new_id()` |
| `layout.rs` | pane 이진 트리 연산(split/close/resize/collect/`check_invariants`). 형제 승격, ratio clamp [0.1,0.9] |
| `state.rs` | `AppState`(store/registry/config_path/injection_paused/automation), `WorkspaceStore`(CRUD, caps, trust), `resolve_inject_target`(라벨→pane, 중복 라벨 거부) |
| `config.rs` | 디스크 로드/저장 + `migrate()`. **스키마 버전별 fixture 테스트가 여기 있음** |
| `session.rs` | `SessionRegistry` + `PtySession`. reader 스레드, ring buffer 적재, spool 적재, idle 추적(`last_output_at`), bracketed-paste 모드 추적, `inject`(게이트 검사), `run_startup`(템플릿 시작 명령) |
| `output.rs` | 16ms 배치 emitter(pty-output/pty-exit 이벤트), ring buffer(1MiB/1024청크, oldest-drop, seq 부여) |
| `commands.rs` | 모든 `#[tauri::command]` + `do_inject` 공유 헬퍼 + `handle_pipe_method`(컨트롤 API 라우팅) + 자동화 폴링 진입점 |
| `automation.rs` | Rule/RuleSource(gitDiff·timer)/Proposal. **순수 로직**(`RuleRuntime::decide*`)과 부수효과 분리 → 단위테스트 용이 |
| `audit.rs` | 주입 감사 로그(JSONL append/tail) |
| `pipe.rs` | named pipe 전송 계층: 인증 핸드셰이크 + 줄 단위 JSON-RPC dispatch. **AppState를 모름**(핸들러 클로저 주입) |
| `spool.rs` | pane 출력 관찰용 per-session 파일 spool(16MiB cap, byte offset 커서) |
| `template.rs` | 템플릿 스키마/변수치환/검증/`build_tree`/`from_pane_tree` |
| `paste.rs` | 클립보드 브리지: `read_clipboard`(arboard) + PNG 인코딩 + paste 파일 저장/prune |
| `bin/bench.rs` | 처리량/지연 벤치마크(BENCHMARK.md) |

### 프론트엔드 `src/`

| 파일 | 역할 |
|---|---|
| `main.ts` | 오케스트레이터: 부팅, 워크스페이스 전환, 커맨드 등록(팔레트 메뉴 전부 여기), 주입/자동화/템플릿 UI 플로우, drag-drop 배선 |
| `terms.ts` | xterm 인스턴스 수명주기, pane 헤더, visual snapshot/replay, **Ctrl+V 가로채기**, paste/drop 브리지 함수 |
| `ipc.ts` | 모든 backend 커맨드의 typed wrapper. **커맨드 추가 시 여기부터** |
| `types.ts` | 공유 타입. 백엔드 serde 모델과 camelCase로 1:1 대응 |
| `renderer.ts` | pane 트리 → DOM(그리드) 렌더 + 리사이즈 핸들 |
| `sidebar.ts` / `palette.ts` / `modal.ts` / `commands.ts` / `themes.ts` | 사이드바, 커맨드 팔레트, 프롬프트/목록 모달, 커맨드 레지스트리, 테마 |
| `autotest.ts` | E2E 시나리오(아래 §4) |

---

## 3. 빌드·테스트·검증 (반드시 이 순서로)

```powershell
# 1) 백엔드 단위+통합 테스트
cd src-tauri; cargo test

# 2) 프론트 타입체크+번들
cd ..; npm run build

# 3) E2E autotest (실제 앱을 띄워 UI를 스크립트로 조작)
Remove-Item "$env:APPDATA\com.terminalf.app\config.json" -ErrorAction SilentlyContinue
Get-Process node,terminal-f -EA SilentlyContinue | Stop-Process -Force
$env:TERMF_AUTOTEST='1'; $env:TERMF_REPORT_PATH="$PWD\autotest-report.json"
npx tauri dev     # 자동 실행 후 스스로 종료, 리포트 JSON 확인 (ok: true)

# 벤치마크(필요 시)
cd src-tauri; cargo run --bin bench -- --soak-secs 600
```

**"완료" 선언 규칙**: cargo test + npm run build + autotest `ok:true`를
전부 확인하기 전에는 완료라고 말하지 않는다. 검증 못 한 항목은 그대로
"미검증"이라고 보고한다.

---

## 4. autotest의 특성과 함정 (중요)

- `TERMF_AUTOTEST=1`이면 앱이 `autotest.ts` 시나리오를 실행하고
  `autotest_report` 커맨드로 JSON을 쓴 뒤 `exit_app`으로 종료한다.
- **비밀폐(non-hermetic)**: config가 실행 간에 누적된다. → 검사는 절대값
  대신 **상대값**(+1 등)으로, 라벨은 `` `x-${Date.now()}` ``로 유일하게.
  깨끗한 시작이 필요하면 실행 전에 config.json 삭제.
- **버퍼 검사 시 줄바꿈 주의**: 긴 텍스트(파일 경로 등)는 터미널 폭에서
  래핑된다. `readBuffer(...).replace(/[\r\n]+/g, "")`로 평탄화 후 매칭.
- **합성 DOM 이벤트의 한계**: 합성 ClipboardEvent/KeyboardEvent는 리스너
  로직만 검증한다. **실제 키 전달 경로(xterm의 keydown 가로채기 등)는 검증
  못 한다.** 이걸로 통과했다고 실기기에서 되는 게 아니다 — ADR-010의 실패
  사례 참조. Chromium은 합성 ClipboardEvent의 `clipboardData` init을
  무시하므로 `Object.defineProperty(ev, "clipboardData", {value: dt})` 필요.
- **클립보드 실붙여넣기 금지**: 사용자 클립보드의 개행 포함 텍스트가 셸에서
  실행될 수 있다. E2E에서는 읽기 전용 검증만.
- `tauri dev`가 자동 종료된 후 **vite(node)가 5173 포트를 물고 남는 경우**가
  있다. 다음 실행 전 `Get-Process node | Stop-Process -Force`.
- 헤드리스로 pwsh를 띄우는 Rust 테스트는 **DSR 핸드셰이크**가 필요하다:
  pwsh가 `ESC[6n`(커서 위치 질의)을 보내고 응답을 기다리며 블록된다 →
  `ESC[1;1R`을 써주는 pump 헬퍼 사용(기존 smoke 테스트 참조).

---

## 5. "X를 추가하려면" 레시피

### 새 팔레트 커맨드
1. `main.ts`의 정적 provider(§`// commands`) 또는 동적 provider에 추가.
2. 팔레트는 결과 무제한 스크롤(캡 넣지 말 것 — 테마 메뉴 실종 사건의 원인).
3. 문서 두 곳에 추가: GUIDE-features-easy.md §6 한 줄 사전 **+**
   GUIDE-command-palette.md에 하는 일/쓰는 법/원리(비개발자용) 상세.

### 새 tauri 커맨드 (프론트↔백엔드)
1. `commands.rs`에 `#[tauri::command]` 함수 (인자는 snake_case — Tauri가
   프론트의 camelCase를 자동 매핑).
2. `lib.rs` `invoke_handler`에 등록 (**빠뜨리면 런타임에서만 실패**).
3. `ipc.ts`에 typed wrapper, 필요 시 `types.ts`에 타입.

### config에 필드 추가 (스키마 마이그레이션)
1. `model.rs`에 `#[serde(default)]` 필드 추가 + `CONFIG_SCHEMA_VERSION` +1.
2. `config.rs::migrate()`의 버전 arm에 이전 버전 추가(additive면 재스탬프만).
3. **이전 버전 fixture 테스트 추가** (기존 v1~v6 테스트 패턴 복사).
4. `PaneLeaf`에 필드를 넣었다면 `layout.rs`/`model.rs`의 모든 생성자 갱신
   (컴파일러가 잡아준다).

### 컨트롤 API(pipe)에 메서드 추가
1. `commands.rs::handle_pipe_method`의 match에 추가.
2. **반드시 기존 게이트 경유**: 주입은 `do_inject`, 관찰은 allow_observe
   검사. 파이프는 전송 계층일 뿐 우회 통로가 되면 안 된다(ADR-008).
3. docs/ADR-008의 메서드 목록과 examples/broker의 README 갱신.

### 주입 경로 추가 (수동/규칙/파이프 외 새 소스)
- 전부 `do_inject(state, …, source)` 하나로 수렴시킨다. 게이트 체인(킬스위치
  → 대상 해석 → allowlist → idle → bracketed paste → audit)을 복제하거나
  일부만 적용하는 코드를 새로 쓰지 말 것(ADR-006).
- 예외: 사용자 본인 타이핑(`write_pane`)과 템플릿 startupCommand는 주입이
  아니므로 게이트/감사 대상이 아니다.

### 새 테마
- `themes.ts`의 THEMES 배열에 추가하면 팔레트에 자동 노출.

### 새 자동화 트리거(RuleSource)
1. `automation.rs`의 `RuleSource` enum에 variant 추가. **주의**: 내부 태그
   enum(`#[serde(tag="type", rename_all="camelCase")]`)은 **variant 필드명을
   rename하지 않는다** — 필드마다 `#[serde(rename="…")]` 명시 + wire-format
   단위테스트 추가(기존 `rule_source_wire_format` 참조).
2. 판단 로직은 `RuleRuntime::decide_*` 순수 함수로 (부수효과 없음 → 테스트).
3. `commands.rs::poll_automation`에 폴링 분기, `main.ts`에 추가 UI.

---

## 6. 디버깅 노하우 (실제로 겪은 것들)

| 증상 | 원인/해법 |
|---|---|
| Ctrl+V에서 브라우저 paste 이벤트가 안 옴 | xterm.js가 keydown을 cancel하고 ^V를 PTY로 보냄. `attachCustomKeyEventHandler`에서 xterm보다 먼저 가로채고 백엔드에서 arboard로 클립보드를 직접 읽는다(ADR-010) |
| HTML5 drop 이벤트가 안 옴 | Tauri가 OS 파일 드롭을 가로챔(dragDropEnabled 기본 true). `getCurrentWebview().onDragDropEvent` 사용. 좌표는 물리픽셀 → `/devicePixelRatio` 후 `elementFromPoint` |
| pipe 서버 핸들러 클로저 타입 불일치(E0631) | bound가 `&mut ConnState`면 클로저 파라미터도 `&mut`로. `&ConnState`를 받는 함수에 넘길 땐 auto-reborrow 됨 |
| `missing field every_ms` 류 serde 오류 | 내부 태그 enum의 `rename_all`은 variant 필드에 적용 안 됨(§5 참조) |
| struct 생성 중 borrow 오류 | lock 결과를 임시 변수로 먼저 꺼낸 뒤 struct literal에 사용 |
| autotest split 검사가 간헐 실패 | config 누적 때문. 상대값 검사로 전환(§4) |
| 앱이 켜지긴 하는데 커맨드 호출이 전부 실패 | `invoke_handler` 등록 누락 의심 |
| 5173 포트 에러로 `tauri dev` 실패 | 좀비 node 프로세스 kill(§4) |
| 헤드리스 pwsh 테스트가 무한 대기 | DSR 핸드셰이크 필요(§4) |
| 팔레트에서 커맨드가 "사라짐" | 표시 개수 캡이 있으면 늦게 등록된 provider가 밀려남. 캡 금지, 스크롤로 해결 |
| `cargo test`가 STATUS_HEAP_CORRUPTION(0xc0000374)로 죽음 | arboard가 Windows OLE 클립보드를 쓰는데 스레드 친화성이 있어, 클립보드를 만지는 테스트 2개 이상이 harness 병렬 스레드에서 동시 실행되면 힙 손상. `paste.rs`의 `CLIPBOARD_LOCK`(Mutex)으로 클립보드 테스트를 직렬화. 새 클립보드 테스트 추가 시 반드시 이 락을 먼저 잡을 것 |
| TUI(클로드코드/vim/tmux) 안에서 복사한 게 클립보드에 안 들어감 | 그 TUI들은 OSC 52(`ESC]52;c;<base64>`)로 클립보드 쓰기를 요청하는데 xterm.js는 OSC 52를 기본 무시함. `terms.ts`에서 `term.parser.registerOscHandler(52, …)`로 디코드→`copy_to_clipboard`. 읽기(`?`)는 거부(클립보드 탈취 방지) |
| 조합 중/직후 Shift+Enter 시 마지막 한글이 다음 줄로 밀림 | ①setTimeout 별도 전송=xterm 조합텍스트 전달과 레이스 ②`onData`에 붙이되 defer를 `ev.isComposing`으로만 판정=**Chromium은 Enter keydown이 compositionend 직후 isComposing=false로 옴** → 즉시전송 경로 탐. **해결: composition 상태 직접 추적**(`compositionstart/end`→`composing`+`awaitingComposedData`), keydown이 조합중/직후면 defer해 `onData`가 확정텍스트 뒤 `data+"\x1b\r"` 원자결합. IME 순서는 합성이벤트로 검증 불가→실기기 필수 |
| split한 새 pane이 원본의 현재 디렉터리가 아니라 처음 열린 곳에서 열림 | pwsh `Set-Location`은 프로세스 CWD 안 바꿈 → OSC 9;9 셸 통합으로 해결(ADR-011). reader가 `scan_cwd`로 `last_cwd` 추적, split_pane이 `pane_live_cwd` 우선. OSC가 화면에 찍히면 프론트 `registerOscHandler(9, d=>d.startsWith("9;"))` 확인. 설치 후 새 pane 필요 |
| cwd 셸통합 설치했는데 split이 여전히 안 따라감 | **프롬프트 함수 안에서 `[Console]::Write`로 OSC를 내보내면 PSReadLine 렌더링 경로에서 PTY로 안 나감**. OSC는 프롬프트의 **반환 문자열에 prepend**해야 함(`return $osc + $base`, WT/VSCode 방식). autotest는 파서(직접 주입)뿐 아니라 프롬프트 반환 방출(`cwdPromptEmit`)까지 검증 |
| 일반 pwsh 프롬프트에서 Ctrl/Shift+Enter 줄바꿈 안 됨 | `\x1b\r`을 pwsh는 ESC(줄 취소)+Enter로 봄. 단 그 시퀀스는 pwsh엔 **Alt+Enter**로 도달하고 Alt+Enter는 언바운드 → opt-in `$PROFILE` 스니펫(shellint.rs, `Set-PSReadLineKeyHandler -Chord 'Alt+Enter' -Function AddLine`)으로 해결. win32-input-mode(WT 방식)는 입력 파이프라인 전면 개편이라 회피. **실전 함정: (1) `$PROFILE`이 OneDrive로 리다이렉트(`OneDrive\문서\PowerShell\...`)되고 폴더가 없을 수 있음 → install은 `create_dir_all` 필수(이미 함). (2) 팔레트 명령을 실제로 실행해야 설치됨(앱 재빌드만으론 안 됨). (3) 설치 후 반드시 **새 pane**을 열어야 프로필 로드됨. 진단: 새 pane에서 `(Get-PSReadLineKeyHandler -Chord Alt+Enter).Function`이 `AddLine`이어야 정상.** |

---

## 7. 런타임 파일 위치 (`%APPDATA%\com.terminalf.app\`)

| 파일/폴더 | 내용 |
|---|---|
| `config.json` | 전체 상태(워크스페이스, pane 트리, ui, automation, trustedRepos). schemaVersion 필드로 마이그레이션 |
| `audit.log` | 주입 감사 로그(JSONL, source 필드로 manual/rule:<id>/pipe:<client> 구분) |
| `control-api.json` | named pipe 이름 + 인증 토큰(앱 기동마다 재생성). 브로커가 읽음 |
| `templates/*.json` | 전역 템플릿(팔레트 "Save current layout"으로 생성) |
| `paste/img-*.png` | 붙여넣은 클립보드 이미지(최근 20개 유지) |
| `spool/*.log` | allowObserve 켜진 pane의 출력 spool(세션 종료 시 삭제) |

repo 로컬: `<repo>/.terminal-f/profile.json` (템플릿, trust 게이트 대상).

---

## 8. 불변식 (깨면 안 되는 것)

1. **pane 트리**: 모든 Split은 자식 정확히 2개, ratio ∈ [0.1, 0.9], pane id
   유일. 트리 변경 후 `check_invariants` 통과해야 함. 마지막 pane은 닫기 거부.
2. **주입 기본 차단**: `allowInjection`/`allowObserve` 기본 false. 새 기능이
   이 기본값을 바꾸면 안 됨.
3. **게이트 우회 금지**: 모든 자동 주입은 `do_inject` 경유(§5).
4. **schemaVersion**: 필드 추가마다 버전 +1과 fixture 테스트. 구버전 config가
   조용히 깨지는 일이 없어야 함.
5. **백엔드 소유권**: 프론트는 레이아웃/세션 상태를 로컬에서 변경하지 않는다.
6. **출력 순서**: ring buffer의 seq는 연속·단조 증가. replay는 seq 기반.

---

## 9. 문서 규칙

- **모든 문서는 한국어로 작성한다** (코드 식별자·파일명·명령어는 영어 유지).
  2026-07-02에 ADR 전체와 ARCHITECTURE.md를 한글화했으므로, 새 ADR도
  한국어로 쓴다.
- 기능 하나 = ADR 하나(`docs/ADR-XXX-*.md`): 배경 → 결정 → 트레이드오프 →
  테스트. **실패했다가 재설계한 경우 그 경위도 기록**(ADR-010처럼).
- 로드맵(PLAN-M1-M2-roadmap.md)의 해당 절에 "구현 완료 + 날짜 + 요약" 추가.
- README에 사용자 관점 요약, GUIDE-features-easy.md에 비개발자 설명(한국어).
  팔레트 커맨드면 GUIDE-command-palette.md에 원리까지 상세 설명 추가.
- 이 문서(DEVELOPMENT.md)의 모듈 지도/레시피/함정 갱신.

## 10. 다음 확장 후보 (로드맵 잔여)

- **UX 다듬기 4건 (기획 완료, 미구현)**: 복사 UX, Ctrl+Enter 멀티라인+IME,
  라이브 cwd 상속(셸 통합), Campbell PowerShell 테마
  → 설계·검증 계획은 [PLAN-UX-polish.md](PLAN-UX-polish.md)
- M1: 브로드캐스트 입력(여러 pane 동시 타이핑), 세션 재연결 고도화
- 템플릿 UX: 폴더 피커 다이얼로그, 템플릿 편집 UI, `${repo}` 자동 일반화
- 컨트롤 API: 브로커별 capability 스코프, rate limit, 이벤트 push(현재 폴링)
- 팔레트: 카테고리 구분/최근 사용 우선 정렬
- 안정화 후: injection/automation/template의 실사용 피드백 반영
