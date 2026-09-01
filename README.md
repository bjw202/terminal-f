# terminal-f

윈도우용 가벼운 터미널입니다. 화면을 여러 칸으로 나눠 쓰고, 프로젝트마다
작업 책상을 따로 두고 오가며, AI 도구(Claude Code, Codex 등)를 여러 개
동시에 띄워 쓰기 좋게 만들었습니다.

터미널에 이미 탭과 화면 분할이 있는데 왜 또 하나일까요? 프로젝트를 여러 개
오가며 AI 도구를 돌리는 사람을 위한 자리가 어디에도 없었기 때문입니다.

- **프로젝트마다 책상 하나** — 탭이 늘어나는 게 아니라 프로젝트가 바뀝니다.
  책상마다 시작 폴더와 색 라벨을 정해 두면 앱을 껐다 켜도 그대로입니다.
- **기존 터미널에서 안 되던 것** — 스크린샷을 `Ctrl+V`로 바로 붙여넣기,
  `Ctrl+Enter`로 긴 프롬프트 줄바꿈, 파일 끌어다 놓기, 조합 중에도 안 깨지는 한글.
- **AI 도구를 나란히** — 한 칸엔 Claude Code, 옆 칸엔 Codex, 그 밑엔 일반 셸.
- **에디터보다 가볍게** — 터미널 하나 쓰자고 VS Code를 통째로 띄우지 않아도 됩니다.

기술 구성은 Tauri 2 + Rust 백엔드(ConPTY) / TypeScript + xterm.js 화면입니다.
터미널 세션과 화면 배치는 전부 백엔드가 관리하고 화면은 보여주기만 합니다.
그래서 다른 책상으로 옮겨도 하던 일이 멈추지 않습니다.

> **처음 읽는 분** → [docs/GUIDE-features-easy.md](docs/GUIDE-features-easy.md)
> (비개발자용 기능 설명 + 전체 메뉴 사전)
> **개발을 이어갈 분** → [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

---

## 세 단어만 알면 됩니다

| 단어 | 뜻 |
|---|---|
| **책상** (워크스페이스) | 프로젝트 하나를 통째로 담는 작업 공간. 프로젝트마다 하나씩 두는 것을 권장합니다. |
| **칸** (페인 · pane) | 책상 위의 화면 분할 칸. 한 칸이 곧 명령창 하나이고, 칸마다 다른 프로그램을 띄울 수 있습니다. |
| **keep-alive** | 다른 책상으로 옮겨도 원래 책상에서 하던 일은 계속 돌아갑니다. 돌아오면 그동안 쌓인 출력이 다 보입니다. |

---

## 필요한 것

- Windows 10 1809 이상 (ConPTY), WebView2 런타임
- Rust(stable), Node.js 18 이상

## 개발 모드로 실행하기

```powershell
npm install
npm run tauri dev        # 개발용 앱 실행 (vite + cargo)
```

## 설치 파일 만들기 (릴리즈 빌드)

배포용 설치 파일을 만들려면 아래 한 줄이면 충분합니다. Tauri가 알아서
프론트엔드 번들(`npm run build`)과 Rust release 컴파일까지 순서대로
실행합니다.

```powershell
npx tauri build          # 프론트엔드 번들 + Rust release 빌드 + NSIS 설치 파일 생성
```

> 단계를 나눠 실행할 수도 있지만, 보통은 위 한 줄로 충분합니다:
```powershell
npm run build            # 프론트엔드 타입검사 + 번들만
cd src-tauri; cargo build           # 백엔드(Rust) 빌드만 (debug)
```

결과물:
- `src-tauri\target\release\terminal-f.exe` — 실행 파일
- `src-tauri\target\release\bundle\nsis\terminal-f_*_x64-setup.exe` — 배포용 설치 파일

## 아이콘 수정 및 업데이트 문제

### 아이콘 다시 만들기

아이콘의 원본은 `src-tauri/icons/icon.png`(512×512) 하나입니다. 모든
크기(`.ico`, `.icns`, PNG 변형, iOS/Android)는 이 원본에서 자동으로
파생됩니다. 그래서 원본 하나를 고친 뒤 아래 명령으로 전체 크기를 다시
만듭니다.

```powershell
npx tauri icon src-tauri/icons/icon.png
```

### 모서리가 흰색으로 나오는 문제 (투명 배경)

이미지 생성 모델(imagegen 등)은 투명 배경(alpha channel)을 직접 만들지
못해 배경이 흰색으로 채워집니다. 이걸 그대로 아이콘으로 쓰면 모서리가
흰색이 됩니다. 원본 PNG를 만든 뒤 배경을 투명하게 후처리(rembg, 이미지
에디터의 '투명하게 지우기' 등)해야 합니다.

### 아이콘을 바꿨는데 예전 아이콘이 계속 보일 때 (캐시)

세 군데 캐시를 모두 끊어야 새 아이콘이 보입니다.

1. **Windows 아이콘 캐시** (가장 흔한 원인):
   ```powershell
   taskkill /f /im explorer.exe
   del /a /q "$env:localappdata\IconCache.db"
   del /a /f /q "$env:localappdata\Microsoft\Windows\Explorer\iconcache_*.db"
   start explorer.exe
   ```

2. **이전 설치 잔재** — 설정 → 앱에서 terminal-f 이전 버전을 완전히
   제거한 뒤 새 설치 파일로 다시 설치합니다.

3. **Rust 빌드 캐시** — 빌드해도 예전 아이콘이 나올 때:
   ```powershell
   cd src-tauri
   Remove-Item -Recurse -Force target\release\bundle   # 번들 캐시만
   # 또는 cargo clean                                   # 전체 정리(느림)
   ```

> 참고: `cargo build`(debug)와 `npx tauri build`(release)는 서로 다른
> 아이콘 캐시 경로를 씁니다. debug로 띄운 뒤 release로 빌드하면
> 작업표시줄에 debug 아이콘이 남아 헷갈릴 수 있으니, 최종 확인은 항상
> release 빌드 + 새로 설치로 하세요.

## 테스트 돌리기

```powershell
cd src-tauri
cargo test               # 단위 테스트 + 실제 셸을 띄우는 스모크 테스트
```

**자동 종합 테스트(autotest)** — 실제 앱 창을 띄워 화면 나누기, 자동 입력,
템플릿 등 32가지를 스스로 검사하고 `autotest-report.json`을 남긴 뒤
종료합니다:

```powershell
$env:TERMF_AUTOTEST='1'; $env:TERMF_REPORT_PATH="$PWD\autotest-report.json"
npx tauri dev   # 끝나면 리포트 파일에서 "ok": true 확인
```

참고: 테스트가 스스로 종료된 뒤에도 vite(node)가 5173 포트를 물고 남아
다음 실행이 실패할 수 있습니다. 그때는 남은 node 프로세스를 종료하고 다시
실행하세요.

## 성능 측정

```powershell
cd src-tauri
cargo run --bin bench -- --soak-secs 600   # bench-report.json 생성
```

방법과 결과: [docs/BENCHMARK.md](docs/BENCHMARK.md)

---

# 사용법

## 처음 5분

1. **책상 만들기** — 왼쪽 사이드바의 `+`를 누르면 새 책상이 생깁니다.
   더블클릭해서 프로젝트 이름으로 바꿔 두세요.
2. **시작 폴더 정하기** — 그 책상을 우클릭 → `Choose folder…`로 프로젝트
   폴더를 고릅니다. 앞으로 그 책상의 모든 칸이 그 폴더에서 열려서, 매번
   `cd`로 찾아 들어갈 일이 없어집니다.
   (처음 지정하면 앱을 완전히 끄고 다시 켠 뒤부터 적용됩니다.)
3. **칸 나누기** — `Ctrl+Shift+D`로 좌우, `Ctrl+Shift+-`로 위아래로 나눕니다.
   한 칸에는 `claude`, 옆 칸에는 `codex`, 아래 칸에는 그냥 셸 — 이런 식으로 씁니다.
4. **오가기** — `Ctrl+1`~`Ctrl+8`로 책상을 바꿉니다. 자리를 옮겨도 원래
   책상의 작업은 계속 돌아갑니다. 안 보는 책상에 새 출력이 오면 파란 점으로,
   프로그램이 끝나면 빨간 점으로 알려 줍니다.
5. **막히면 `Ctrl+Shift+P`** — 명령 팔레트를 열어 몇 글자만 치면 모든 기능을
   찾아 실행할 수 있습니다.

## 단축키

| 키 | 동작 |
|---|---|
| `Ctrl+Shift+D` | 지금 칸을 좌/우로 나누기 |
| `Ctrl+Shift+-` | 지금 칸을 위/아래로 나누기 |
| `Ctrl+Shift+W` | 지금 칸 닫기 (마지막 한 칸은 안 닫힘) |
| `Ctrl+Shift+Z` | 지금 칸 전체화면 확대 ↔ 원래대로 |
| `Ctrl+Shift+P` | 명령 팔레트 (모든 기능 검색 실행) |
| `Ctrl+Shift+B` | 사이드바 접기/펴기 (그냥 `Ctrl+B`는 셸에 양보 — readline/tmux용) |
| `Ctrl+1~8` | 1~8번 책상으로 즉시 이동 |

## 사이드바 — 책상 다루기

책상은 전부 왼쪽 사이드바에서 관리합니다. 접어 두면 화면을 거의 차지하지
않습니다(`Ctrl+Shift+B`).

| 조작 | 하는 일 |
|---|---|
| 클릭 | 그 책상으로 이동 |
| 더블클릭 | 이름 변경 |
| 우클릭 | 색 라벨 지정 · 시작 폴더 지정 |
| 드래그 | 순서 바꾸기 |
| `×` | 삭제 (그 책상의 세션도 함께 종료) |
| `+` | 새 책상 만들기 |

배지의 색이 그 책상의 색 라벨입니다. **파란 점**은 안 보는 책상에 새 출력이
왔다는 뜻, **빨간 점**은 어떤 칸의 프로그램이 끝났다는 뜻입니다.
테마와 글자 크기는 팔레트에서 바꿉니다 (`Theme: …`, `View: … font size`).

## 스크린샷 붙여넣기 & 파일 끌어다 놓기

윈도우 터미널들은 `Ctrl+V`에서 클립보드의 **글자만** 전달해서, 스크린샷을
Claude Code에 붙여넣어도 아무 일이 안 일어납니다. terminal-f는 이걸
해결했습니다 ([ADR-010](docs/ADR-010-image-paste-bridge.md)):

- 클립보드에 **글자**가 있으면 → 평소처럼 글자 붙여넣기 (달라지는 것 없음)
- 클립보드에 **이미지**만 있으면 → 파일로 저장하고 **그 파일 경로**를
  붙여넣기. Claude Code는 이미지 경로를 받으면 이미지를 첨부한 것으로
  처리합니다. (저장 위치는 앱 설정 폴더의 `paste\`, 최근 20개 유지)
- **탐색기에서 파일을 칸 위로 끌어다 놓으면** 그 파일의 경로가 입력됩니다.

써보기: `Win+Shift+S`로 캡처 → Claude Code 칸 클릭 → `Ctrl+V` → 전송.

참고: `Ctrl+V`를 붙여넣기 전용으로 쓰기 때문에, 터미널 안의 프로그램에는
`^V` 키가 전달되지 않습니다 (Windows Terminal과 같은 방식).

## 복사 · 줄바꿈 · 링크 열기

- **복사** — 드래그해서 선택하는 것만으로 바로 복사됩니다(기본 켜짐).
  선택이 있을 때 `Ctrl+C`, 선택이 없을 때도 항상 복사는 `Ctrl+Shift+C`.
  선택이 없을 때 `Ctrl+C`는 평소처럼 명령 중단으로 동작합니다.
  우클릭은 선택이 있으면 복사, 없으면 붙여넣기입니다.
  자동 복사가 불편하면 팔레트의 `Copy: Disable copy-on-select`로 끕니다.
- **줄바꿈** — Enter가 곧 전송인 도구에서 `Ctrl+Enter`(또는 `Shift+Enter`)로
  줄을 바꿔 긴 프롬프트를 나눠 쓸 수 있습니다. 한글 조합 중에 눌러도
  글자가 먼저 완성된 뒤에 줄이 바뀝니다.
- **링크 열기** — 출력에 나온 `http`/`https` 주소를 `Ctrl+클릭`하면
  브라우저에서 열립니다(기본 켜짐, 팔레트 `Links: …`로 끌 수 있음).
  주소는 열기 전에 백엔드에서 한 번 검사합니다
  ([ADR-012](docs/ADR-012-url-open-security.md)).

## 명령 팔레트

`Ctrl+Shift+P`를 누르면 뜨는 만능 검색창입니다. 테마 바꾸기, 글자 크기,
사이드바, 복사 설정은 물론이고 아래의 템플릿·자동화·자동 입력 같은 고급
기능까지 전부 여기서 몇 글자만 쳐서 찾아 실행합니다. 전체 메뉴 목록은
[docs/GUIDE-command-palette.md](docs/GUIDE-command-palette.md)에 있습니다.

## 한글 입력 안정화

Claude Code처럼 화면에 글자를 계속 쏟아내는 프로그램 위에서 한글을 치면,
글자를 조합하는 도중(자음·모음을 모으는 중)에 화면 갱신이 끼어들어
**완성 글자가 깨지는** 문제가 있었습니다. 이제 글자를 조합하는 동안에는
그 칸의 화면 출력을 잠시 붙잡아 뒀다가 글자가 완성되면 한꺼번에
보여줍니다. 단, **방금 입력한 글자가 화면에 찍히는 것은 미루지 않아서**
타이핑은 평소처럼 즉시 반응하고, 프로그램이 쏟아내는 출력만 아주
잠깐(최대 1초) 미뤄집니다 — 체감 차이는 없습니다.

함께 고친 것: 한글 조합 도중 다른 곳을 클릭하면 드물게 `Ctrl+C`/`Ctrl+V`
단축키가 먹통이 되던 문제.

---

# 더 쓸 수 있는 기능

아무것도 켜지 않으면 일반 터미널과 똑같이 동작합니다. 아래는 필요할 때만
켜서 쓰는 기능이고, 위험할 수 있는 것은 안전장치가 전부 기본으로 켜져 있습니다.

## 워크스페이스 시작 폴더

책상(워크스페이스)마다 **시작 폴더**를 지정해 둘 수 있습니다
([ADR-013](docs/ADR-013-workspace-root-folder.md)). 사이드바에서 책상을
우클릭 → "Choose folder…"로 폴더를 고르면, 그 책상의 모든 칸이 다음부터는
그 폴더에서 셸을 엽니다(앱을 완전히 종료했다가 다시 켤 때부터 적용).
지정하지 않으면 지금까지와 동일하게 동작합니다.

## 템플릿(매크로 레시피)

책상 세팅(칸 나누기 + 폴더 이동 + 프로그램 실행)을 레시피로 저장해 두고 한
번에 재현합니다 ([ADR-009](docs/ADR-009-project-templates.md)).

- `Template: Apply "이름"` — 레시피 실행, 완성된 새 책상이 뜸
- `Template: Save current layout as template` — 지금 배치를 레시피로 저장
- `Template: Apply repo profile` — 프로젝트 폴더 안의 공유 레시피
  (`.terminal-f/profile.json`) 실행. 자동 실행 명령이 들어 있으면 **신뢰
  확인**을 먼저 거칩니다 (낯선 프로젝트의 명령이 몰래 돌지 않도록).

레시피의 시작 명령(`startupCommand`)은 셸 안에서 실행되므로 명령이 끝나도
칸은 살아있는 명령창으로 남습니다. 예제: `examples/templates/`

## 자동 입력(주입) — 안전장치가 기본

자동화가 특정 칸에 대신 타이핑해 주는 기능의 토대입니다. 위험할 수 있는
기능이라 안전장치가 전부 기본으로 켜져 있습니다
([ADR-006](docs/ADR-006-injection-safety.md)):

- 칸마다 **허용 스위치**가 있고 기본은 꺼짐 (켜면 칸 이름표에 ⚡)
- 대상 칸이 일하는 중이면 **조용해질 때까지 대기** (1.5초 규칙)
- 전체를 즉시 멈추는 **비상 정지 스위치**
- 누가 언제 뭘 입력했는지 남는 **기록장(감사 로그)**

팔레트 메뉴: `Injection: Allow/disallow …`, `Pane: Edit labels …`,
`Injection: Send prompt …`, `Injection: Pause all`, `… Show audit log`

## 자동화 규칙(감시원)

"코드가 바뀌면 → Codex 칸에 리뷰 요청을 보내라" 같은 약속을 등록합니다
([ADR-007](docs/ADR-007-automation-rule-engine.md)). 규칙이 발동해도 기본은
[승인]/[무시] 확인 창이 먼저 뜹니다. 폴더 감시(git)와 타이머(N분마다) 두
종류가 있습니다.

팔레트 메뉴: `Automation: Add git-review rule`, `… Add timer rule`,
`… List rules`, `… Run rule now`, `… Enable/Disable`, `… Remove`

## 외부 프로그램 연결 (컨트롤 API, 개발자용)

직접 만든 감시원 프로그램(브로커)이 terminal-f를 읽고 조작할 수 있는 공식
통로입니다 ([ADR-008](docs/ADR-008-control-api-named-pipe.md)). 앱이 켜질 때
접속 주소와 비밀 토큰을 `control-api.json`에 남기고, 브로커는 그걸 읽어
접속합니다. 무엇을 하든 위의 안전장치(허용 스위치, 기록장 등)를 그대로
통과해야 하며 우회할 수 없습니다.

- 칸의 화면 출력을 읽으려면 그 칸의 **관찰 허용 스위치**(팔레트
  `Observe: …`, 켜면 👁)를 먼저 켜야 합니다. 화면에는 비밀번호 같은 것이
  지나갈 수 있어 기본은 꺼짐입니다.
- 실행 가능한 예제 브로커: `examples/broker-git-review/` — 코드 변경을
  감지하면 `claude -p`(헤드리스)로 리뷰를 만들어 지정한 칸에 넣어줍니다.

---

## 문서

- [docs/GUIDE-intro.html](docs/GUIDE-intro.html) — 그림으로 보는 사용법 안내
  (브라우저로 열어 보세요)
- [docs/GUIDE-features-easy.md](docs/GUIDE-features-easy.md) — 비개발자용
  기능 가이드 + 전체 팔레트 메뉴 사전
- [docs/GUIDE-command-palette.md](docs/GUIDE-command-palette.md) — 명령
  팔레트 전체 목록
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) — **개발을 이어가려면 여기부터**:
  모듈 지도, 빌드/테스트 절차, 기능 추가 레시피, 디버깅 노하우, 불변식
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 시스템 설계
- [docs/PLAN-M1-M2-roadmap.md](docs/PLAN-M1-M2-roadmap.md) — 기능 로드맵과
  구현 현황
- docs/ 안의 ADR-001~014 — 기능별 설계 결정 기록
- [docs/BENCHMARK.md](docs/BENCHMARK.md) — 성능 측정 방법/결과
