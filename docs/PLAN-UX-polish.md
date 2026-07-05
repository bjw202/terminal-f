# 기획: UX 다듬기 (복사·멀티라인·cwd 상속·테마)

> 2026-07-02 기획. **①②③④ 전부 구현·검증 완료 (2026-07-05~06).** 각 항목
> 제목의 ✅ 블록에 최종 구현 노트가 있다. ③는 [ADR-011], 복사/멀티라인 셸
> 통합은 [ADR-010] 참조.

진행 완료: **①복사** (스마트 Ctrl+C/Ctrl+Shift+C/우클릭/copy-on-select/OSC 52)
→ **④Campbell 테마** → **②멀티라인+IME** (onData 원자결합 + Alt+Enter opt-in)
→ **③라이브 cwd** (OSC 9;9 셸 통합).

---

## ① 화면 텍스트 복사가 안 됨 ✅ 구현됨 (2026-07-05)

> **구현 완료.** 백엔드 `copy_to_clipboard`(arboard `set_text`), 프론트
> 스마트 Ctrl+C / Ctrl+Shift+C / 우클릭 복사, copy-on-select 팔레트 토글
> (`uiPrefs.copyOnSelect`, 기본 꺼짐)까지 아래 설계대로 반영. autotest에
> `copyRoundTrip` 검사(프로그래매틱 selectAll → 복사 → paste_clipboard 왕복,
> 클립보드 원상 복원) 추가. paste.rs에 `write_then_read_round_trips_text`
> 단위 테스트 추가 — 단, arboard가 Windows OLE 클립보드의 스레드 친화성
> 때문에 클립보드 테스트 2개를 동시 실행하면 힙 손상(STATUS_HEAP_CORRUPTION)이
> 나므로 `CLIPBOARD_LOCK`(Mutex)으로 직렬화함. **Shift+드래그(마우스 트래킹
> TUI 위 강제 선택)는 xterm 내장이며 실기기 확인 항목으로 남음.**
>
> **후속 수정 (2026-07-05) — OSC 52 클립보드 쓰기.** 실기기에서 "Claude Code
> 안에서 복사한 것은 안 붙고 이전 클립보드가 붙는다" 보고. 원인: Claude
> Code 같은 TUI는 자체 복사에서 `ESC]52;c;<base64>` (OSC 52) 시퀀스를 출력해
> 클립보드 갱신을 요청하는데, xterm.js는 클립보드 바인딩이 없어 OSC 52를
> 기본 무시 → TUI 복사가 OS 클립보드에 도달 못 함. 해결: `terms.ts`에서
> `term.parser.registerOscHandler(52, …)` 등록 → payload base64(UTF-8) 디코드
> 후 `copy_to_clipboard`로 씀. **읽기 요청(payload `?`)은 거부**(pane 내
> 프로세스의 클립보드 탈취 방지), 8MiB 초과 payload 무시. autotest
> `osc52Copy` 검사(`ESC]52;c;<b64>BEL` write → paste_clipboard 확인) 추가.

### 현상

- 일반 셸에서 드래그로 선택은 되지만 복사할 방법이 없다
  (Ctrl+C는 ^C(SIGINT)로 셸에 전달되는 게 맞으므로 복사가 아님).
- **클로드코드 실행 중에는 드래그 선택 자체가 안 된다.**

### 원인

1. 복사 단축키/우클릭 동작을 아무것도 바인딩하지 않았다.
2. 클로드코드 같은 TUI는 **마우스 트래킹 모드**를 켠다 → xterm.js가 마우스
   이벤트를 선택이 아니라 앱(TUI)으로 전달한다. 선택이 "안 되는" 게 아니라
   앱이 가져가는 것.
3. WebView의 `navigator.clipboard.writeText`는 권한/포커스 이슈가 있을 수
   있다 → 백엔드 arboard(이미 의존성 있음)로 쓰는 것이 결정적.

### 설계 (Windows Terminal 관례를 따름)

| 입력 | 동작 |
|---|---|
| **선택이 있을 때 Ctrl+C** | 복사 + 선택 해제 (^C를 보내지 않음) |
| 선택이 없을 때 Ctrl+C | 기존대로 ^C 전달 |
| **Ctrl+Shift+C** | 항상 선택 복사 |
| **우클릭** | 선택 있으면 복사, 없으면 붙여넣기(paste_clipboard 재사용) |
| **Shift+드래그** | 마우스 트래킹 TUI(클로드코드) 위에서 선택 강제 — xterm 내장 동작. 문서화 필수 |
| copy-on-select | 옵션(팔레트 토글, uiPrefs 저장). 기본 꺼짐 |

- 백엔드: `copy_to_clipboard(text)` 커맨드 신설 (arboard `set_text`).
- 키 처리 위치: 기존 `attachCustomKeyEventHandler`(terms.ts) — Ctrl+V와 같은
  자리에서 Ctrl+C(선택 검사), Ctrl+Shift+C 분기.
- 우클릭: pane 요소 `contextmenu`에서 preventDefault 후 분기.

### 검증

- autotest: `term.select()`로 프로그래매틱 선택 → 스마트 Ctrl+C 분기 함수
  호출 → `copy_to_clipboard` 후 `paste_clipboard`로 왕복 확인(클립보드
  원상 복원 — paste.rs 테스트의 저장/복원 패턴 재사용).
- 실기기: 클로드코드 켠 채 Shift+드래그 → Ctrl+C → 다른 곳에 붙여넣기.

### 리스크

- ^C에 의존하는 습관(복사하려고 선택해 둔 상태에서 실행 중단하려고 Ctrl+C)
  → 선택이 있으면 중단이 안 됨. Windows Terminal과 동일한 트레이드오프이며
  두 번 누르면 됨(첫 번째가 선택 해제). 문서화.
- 마우스 트래킹 중 Shift+드래그가 xterm 버전에 따라 동작 차이 가능 → 실기기
  확인 필수.

---

## ② Ctrl+Enter 멀티라인 입력 + 한글 IME 안정성 ✅ 구현·검증 완료 (2026-07-05)

> **구현 완료.** `terms.ts` 키 핸들러에서 Ctrl+Enter / Shift+Enter → PTY에
> `\x1b\r`(ESC+CR) 전송. 코드 결정 로직은 순수 함수 `newlineChordFor(ev)`로
> 분리해 autotest `multilineChord`(send/defer/null 7케이스) 검증.
>
> **실기기 후속 수정 (2026-07-05) — 두 가지 버그. IME는 2차까지 고침.**
>
> **한글 IME 순서 버그 (Claude Code)**: 조합 중 Shift+Enter 시 마지막 글자가
> 다음 줄로 밀림. **1차 시도(실패)**: `compositionend`+setTimeout으로 줄바꿈을
> 따로 보냄 → xterm 조합텍스트 전달과 레이스. **2차 시도(부분·실패)**:
> `pendingNewline`을 `onData`에서 소비해 확정텍스트 뒤에 붙였으나, `defer` 판정을
> `ev.isComposing`에만 의존 → **Chromium에선 Enter keydown이 compositionend
> *직후*(isComposing=false)에 도착**해 "즉시 전송" 경로를 타서 여전히 줄바꿈이
> 앞섬. **3차(최종)**: 조합 상태를 **직접 추적**(`compositionstart/end`로
> `composing` + `awaitingComposedData` 플래그). 뉴라인 코드 판정:
> ①조합 중이면 defer+키 통과(IME 확정 유도) ②조합 방금 끝남(awaitingComposedData)
> 이면 Enter를 preventDefault로 눌러 삼키고 defer ③조합 아니면 즉시 전송. defer된
> 줄바꿈은 항상 `onData`가 확정텍스트 뒤에 원자 결합(`data + "\x1b\r"`).
> `awaitingComposedData`는 onData가 클리어(정상) 또는 120ms 폴백(조합 중단).
> **실제 IME 조합 순서는 합성 이벤트로 검증 불가(ADR-010) → 실기기 확인 필수.**
>
> **일반 pwsh 프롬프트에서 줄바꿈 안 됨**: `\x1b\r`을 pwsh는 ESC(줄 취소)
> +Enter로 해석. **진단 결과 PSReadLine은 이미 Shift+Enter=AddLine을 네이티브
> 지원**하지만, VT 터미널은 Shift+Enter 키 이벤트 자체를 전송할 수 없는 게
> 문제(Windows Terminal은 win32-input-mode로 해결 — 입력 파이프라인 전면 개편
> 필요, 고위험). **더 간단한 해법 채택**: 우리가 보내는 `\x1b\r`이 pwsh엔
> **Alt+Enter**로 도달함(autotest로 실측: `42+`⏎`58`=100). Alt+Enter는 기본
> 언바운드 → opt-in으로 `$PROFILE`에 `Set-PSReadLineKeyHandler -Chord
> 'Alt+Enter' -Function AddLine` 한 줄 설치(shellint.rs, 스니펫 전문 표시+확인
> 후 기록, 멱등, fence로 제거 가능, `Import-Module PSReadLine`로 로드 보장,
> 재설치 시 기존 블록 교체). 팔레트 "Shell: Enable multiline in PowerShell
> (Ctrl+Enter)". 입력 파이프라인 무변경, 저위험, 되돌리기 쉬움. 셸은
> `-NoProfile` 없이 실행하므로 **새로 여는** pwsh pane은 프로필을 로드함(설치 후
> 기존 pane은 재시작 필요). 클로드코드 내부에서는 `\x1b\r`이 이미 줄바꿈이라
> 설치 불필요. **주의: 설치 후 새 pane에서 `Get-PSReadLineKeyHandler -Chord
> Alt+Enter`가 `AddLine`을 반환해야 정상.**

### 현상

클로드코드 입력창에서 줄바꿈을 넣을 수 없다 — Enter는 곧바로 제출된다.
한글 조합 중 특수키를 누르면 마지막 글자가 유실되거나 미완성으로 들어가는
위험도 있다.

### 원인

1. xterm.js는 Enter를 수식키와 무관하게 항상 `\r`로 보낸다. TUI 입장에서는
   Enter와 Shift/Ctrl+Enter를 구분할 수 없다.
2. 클로드코드는 **ESC+CR**(`\x1b\r`, Meta+Enter)을 "줄바꿈 삽입"으로
   해석한다 (VS Code 터미널의 `/terminal-setup`이 Shift+Enter를 이렇게
   매핑하는 것과 같은 규약).
3. 한글 IME 조합 중에는 keydown이 `isComposing=true`(keyCode 229)로 온다.
   이 상태에서 키를 가로채 PTY로 보내면 조합 확정 전 문자가 유실된다.

### 설계

- 키 핸들러(terms.ts)에서 **Ctrl+Enter와 Shift+Enter → PTY에 `\x1b\r` 전송**.
- **IME 규칙 (핵심)**:
  1. `ev.isComposing || ev.keyCode === 229`이면 절대 가로채지 않는다
     (조합은 IME/xterm 조합 헬퍼에 맡김).
  2. 조합 중 Ctrl+Enter가 눌리면 `pendingNewline` 플래그만 세운다.
  3. textarea의 `compositionend`에서 조합 확정 직후 플래그를 소비해
     `\x1b\r`을 보낸다 → **"글자 완성 → 정확히 다음 줄" 순서 보장**.
- 상태바 힌트: 클로드코드 라벨 pane 포커스 시 "Ctrl+Enter=줄바꿈" 안내(선택).

### 검증

- autotest: onData 후킹 → 합성 keydown(Ctrl+Enter) 디스패치 → 페이로드가
  `\x1b\r`인지. 합성 compositionstart → Ctrl+Enter → compositionend 순서로
  pending 로직 검증. (합성 이벤트는 리스너 로직 검증용이라는 한계를 명심 —
  ADR-010 교훈. **실기기 한글 조합 테스트를 완료 조건에 포함**.)
- 실기기: 클로드코드에서 "안녕하세요" 조합 중(마지막 글자 미확정 상태)
  Ctrl+Enter → 글자 온전히 입력되고 커서가 다음 줄로.

### 리스크

- pwsh(PSReadLine)에서 ESC는 "입력 지우기"다 → 셸 프롬프트에서 Ctrl+Enter를
  누르면 입력이 지워질 수 있음. pane의 포그라운드 앱을 감지할 방법이 없어
  v1은 문서화로 처리. (필요 시 향후: pane별 "멀티라인 키 끄기" 토글.)
- IME 구현은 브라우저/버전별 편차가 큼 — WebView2(Chromium) 기준으로만
  보장하고 명시.

---

## ③ split 시 새 페인이 원본 페인의 cwd에서 열리기 ✅ 구현·검증 완료 (2026-07-06)

> **구현 완료 — 상세는 [ADR-011](ADR-011-shell-integration-live-cwd.md).**
> 백엔드 reader가 `OSC 9;9;<path>`(+OSC 7) 파싱 → `PtySession.last_cwd`
> (순수 함수 `scan_cwd` + 청크 조립 버퍼 + 단위 테스트 6개). `split_pane`은
> `pane_live_cwd`가 실제 디렉터리이면 그것을, 없으면 leaf.cwd로 폴백. 프론트
> `registerOscHandler(9, …)`로 OSC 9;9 표시 억제. 셸 통합 설치는 shellint.rs
> `cwd_snippet`(프롬프트 래퍼, 기존 prompt 체이닝) + 팔레트 "Shell: Enable
> live directory tracking" (confirmModal 확인, 멀티라인과 같은 shellint 인프라
> 공유). autotest `liveCwdSplit`: 셸이 OSC 9;9(C:\Windows)를 출력 → split →
> 새 pane이 C:\Windows에서 열림을 실 PTY로 검증. **cmd.exe 미지원, 설치 후
> 새 pane 필요.**

### 현상과 현재 상태

split은 이미 원본 leaf의 `cwd` 필드를 상속한다(commands.rs `split_pane`).
그러나 이 값은 **페인 생성 시점의 경로**다. 사용자가 셸에서 `cd`로 이동한
**라이브 경로**는 백엔드가 모른다 — 이것이 진짜 문제.

### 원인 (Windows 특성)

- **PowerShell의 `Set-Location`은 프로세스 CWD를 바꾸지 않는다**(런스페이스
  별 위치를 내부 관리). 따라서 자식 프로세스 CWD 조회(PEB 읽기)로는 pwsh의
  현재 위치를 알 수 없다.
- Windows Terminal의 "현재 디렉터리에서 탭 복제"도 같은 문제를 **셸 통합
  (OSC 9;9)** 으로 해결한다: 셸이 프롬프트마다 자기 위치를 이스케이프
  시퀀스로 출력하고 터미널이 이를 파싱한다.

### 설계

1. **OSC cwd 파싱**: 세션 reader 스레드에서 출력 스트림의
   `ESC]9;9;<경로>ESC\`(ConEmu/WT 규약)와 `ESC]7;file://…`(범용)를 파싱해
   `PtySession.last_cwd`를 갱신한다.
   - 시퀀스가 읽기 청크 경계에 걸릴 수 있음 → 기존 UTF-8 pending 버퍼와
     같은 방식의 조립 버퍼 필요.
2. **split_pane**: `last_cwd`가 있으면 그것을, 없으면 현행대로 leaf의 cwd를
   사용 (동작 저하 없음, 점진 개선).
3. **셸 통합 설치 커맨드**: 팔레트 "Shell integration: pwsh 프로필에 설치"
   — `$PROFILE`에 prompt 래퍼(위치를 OSC 9;9로 출력) 스니펫을 추가. 설치
   전에 **스니펫 전문을 보여주고 확인**받는다(사용자 프로필 파일 수정이므로).
   이미 설치돼 있으면 감지하고 건너뜀. 제거 방법도 문서화.
4. 보너스(선택): 워크스페이스 전환/앱 종료 시 `last_cwd`를 leaf.cwd에
   반영해 config에 저장 → **재시작 후에도 마지막 위치에서 시작**.
5. cmd.exe는 미지원으로 문서화 (cmd는 cd가 프로세스 CWD를 바꾸므로 향후
   PEB 폴백 검토 가능).

### 검증

- 단위: OSC 파서 — 완전한 시퀀스, 청크 분할 시퀀스, 잘못된 시퀀스.
- E2E(autotest): 셸에서 OSC 9;9를 직접 출력시킬 수 있다
  (`Write-Host "$([char]27)]9;9;C:\test$([char]27)\"`) → split → 새 페인의
  cwd가 `C:\test`인지 확인. **실제 파싱 경로를 실기기 없이 검증 가능**.
- 실기기: 프로필 설치 → cd 이동 → split → 새 페인 위치 확인.

### 리스크

- 프로필 수정은 사용자 환경 변경 — 반드시 opt-in + 전문 표시 + 확인.
- oh-my-posh/starship 등 기존 프롬프트 커스터마이징과의 충돌 → 스니펫을
  기존 prompt 함수를 래핑(체이닝)하는 형태로 작성해 공존시킨다.
- 경로에 `]`/비ASCII 포함 시 파싱 주의 (OSC 종료는 ST(`ESC\`) 또는 BEL).

---

## ④ Campbell PowerShell 테마 추가 ✅ 구현됨 (2026-07-05)

> **구현 완료.** `themes.ts` THEMES에 `campbell-powershell`("Campbell
> PowerShell (navy)") 추가 — 팔레트에 자동 노출. ANSI 16색은 아래 WT 공식
> 팔레트, 앱 크롬(사이드바/패널/상태바)은 남색 배경 `#012456`에 맞춰 파생.
> selectionBackground는 밝은 텍스트 가독성을 위해 남색 계열 `#264f78` 채택.

Windows Terminal 기본 배색의 PowerShell 변형(남색 배경 `#012456`)을
themes.ts THEMES에 추가한다. 공식 팔레트:

| 항목 | 값 |
|---|---|
| background / foreground | `#012456` / `#CCCCCC` |
| black / red / green / yellow | `#0C0C0C` `#C50F1F` `#13A10E` `#C19C00` |
| blue / magenta / cyan / white | `#0037DA` `#881798` `#3A96DD` `#CCCCCC` |
| bright 계열 | `#767676` `#E74856` `#16C60C` `#F9F1A5` `#3B78FF` `#B4009E` `#61D6D6` `#F2F2F2` |

- 앱 UI 색(사이드바/패널/상태바)은 배경 `#012456`에 어울리는 남색 계열로
  파생(기존 테마들의 UI 필드 구조 그대로).
- selectionBackground는 `#FEDBA9` 계열(WT 기본 선택색) 또는 반투명 파랑 중
  가독성 확인 후 결정.
- 작업량: THEMES 배열에 1개 항목 추가 — 팔레트에 자동 노출. ADR 불필요.

---

## 추가 가치 제안 (이번 범위 밖, 다음 후보)

우선순위를 붙인 백로그. 사용자 확인 후 착수할 것.

1. **버퍼 검색** (`Ctrl+Shift+F`, xterm search addon) — 긴 로그에서 필수.
2. **셸 통합 확장** — ③의 OSC 기반 위에: 실행 중 명령을 페인 헤더에 표시,
   장시간 명령 완료 알림(비활성 워크스페이스면 사이드바 배지).
3. ~~**URL 클릭** (web-links addon) — 출력의 링크를 Ctrl+클릭으로 열기.~~
   ✅ **구현·검증 완료 (2026-07-06, [ADR-012](ADR-012-url-open-security.md))**.
   web-links addon으로 http/https 링크화, Ctrl/Cmd+클릭 시 백엔드
   `open_external_url`(스킴 화이트리스트 + tauri-plugin-opener ShellExecute)로
   기본 브라우저에서 열기. 팔레트 "Links: …Ctrl+click to open URLs"로 토글
   (기본 ON). Rust `is_safe_external_url` 단위 테스트 + autotest
   `urlOpenGate`/`urlRejectsUnsafe`.
4. **브로드캐스트 입력** (로드맵 M1 잔여) — 여러 페인에 동시 타이핑.
5. **폰트 패밀리 설정** — 현재 크기만 조절 가능.
