# terminal-f

윈도우 전용 터미널 프로그램입니다. 화면을 여러 칸으로 나눠 쓰고(분할 페인),
프로젝트별 작업 책상(워크스페이스)을 여러 개 두고 오가며, AI 도구
(Claude Code, Codex 등)를 여러 개 동시에 띄워 쓰기 좋게 만들었습니다.

- 기술 구성: Tauri 2 + Rust 백엔드(ConPTY) / TypeScript + xterm.js 화면
- 핵심 원칙: 터미널 세션과 화면 배치는 전부 백엔드가 관리하고, 화면은
  보여주기만 합니다. 그래서 다른 책상으로 옮겨도 하던 일이 멈추지 않습니다.

> **처음 읽는 분** → [docs/GUIDE-features-easy.md](docs/GUIDE-features-easy.md)
> (비개발자용 기능 설명 + 전체 메뉴 사전)
> **개발을 이어갈 분** → [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

---

## 필요한 것

- Windows 10 1809 이상 (ConPTY), WebView2 런타임
- Rust(stable), Node.js 18 이상

## 개발 모드로 실행하기

```powershell
npm install
npm run tauri dev        # 개발용 앱 실행 (vite + cargo)
```

## 설치 파일 만들기

```powershell
npm run build            # 프론트엔드 타입검사 + 번들
cd src-tauri; cargo build           # 백엔드 디버그 빌드
npx tauri build          # 배포용 설치 파일(NSIS) 생성
```

## 테스트 돌리기

```powershell
cd src-tauri
cargo test               # 단위 테스트 + 실제 셸을 띄우는 스모크 테스트
```

**자동 종합 테스트(autotest)** — 실제 앱 창을 띄워 화면 나누기, 자동 입력,
템플릿 등 22가지를 스스로 검사하고 `autotest-report.json`을 남긴 뒤
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

**사이드바**: 클릭=이동, 더블클릭=이름 변경, 우클릭=색 라벨, 드래그=순서
변경, `×`=삭제(그 책상의 세션도 종료), `+`=새로 만들기.
파란 점=안 보는 책상에 새 출력 도착, 빨간 점=어떤 칸의 프로그램 종료.
테마와 글자 크기는 팔레트에서 (`Theme: …`, `View: … font size`).

---

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

---

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

- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) — **개발을 이어가려면 여기부터**:
  모듈 지도, 빌드/테스트 절차, 기능 추가 레시피, 디버깅 노하우, 불변식
- [docs/GUIDE-features-easy.md](docs/GUIDE-features-easy.md) — 비개발자용
  기능 가이드 + 전체 팔레트 메뉴 사전
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — 시스템 설계
- [docs/PLAN-M1-M2-roadmap.md](docs/PLAN-M1-M2-roadmap.md) — 기능 로드맵과
  구현 현황
- docs/ 안의 ADR-001~010 — 기능별 설계 결정 기록
- [docs/BENCHMARK.md](docs/BENCHMARK.md) — 성능 측정 방법/결과
