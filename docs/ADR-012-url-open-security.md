# ADR-012: 출력 URL 클릭 열기와 보안

상태: 채택 (UX 백로그 ③, 2026-07-06)

## 맥락

터미널 출력의 `https://…` 링크를 Ctrl+클릭으로 기본 브라우저에서 열고 싶다
(VS Code 터미널과 같은 UX). xterm의 **web-links addon**이 플레인 텍스트 URL을
정규식으로 인식해 클릭 가능하게 만든다.

그러나 **링크를 여는 것은 "터미널 내용(신뢰 불가)"을 근거로 외부 프로그램
(브라우저·프로토콜 핸들러)을 실행하는 행위**다. 악성 프로그램이 출력에
`javascript:`, `file:///…calc.exe`, `vscode://…` 같은 문자열을 찍어 사용자가
클릭하게 유도할 수 있다. 또 URL을 셸(`cmd /c start <url>`)로 넘기면
쿼리스트링의 `&`가 명령 구분자로 해석되어 인젝션이 가능하다.

## 결정

1. **스킴 화이트리스트(백엔드)**: 순수 함수 `is_safe_external_url`(commands.rs)
   가 **http/https만 허용**하고 그 외(`javascript:`·`file:`·`data:`·`ftp:`·
   스킴상대 `//…` 등)와 **제어문자·공백**, 4096자 초과를 전부 거부한다.
   프론트가 아니라 백엔드에서 검증한다(신뢰 경계는 백엔드). 순수 함수라 단위
   테스트로 허용/거부 표를 고정한다.

2. **셸을 거치지 않는 opener**: 검증 후 **tauri-plugin-opener**의 Rust API
   `app.opener().open_url(url, None)`로 연다. Windows에서 `ShellExecuteW`를
   쓰므로 쿼리 `&`가 포함된 정상 URL도 안전하며, `cmd /c start`의 인자 인젝션
   표면이 없다. (`open` 크레이트는 과거 `cmd /c start` 경로라 `&` 문제가 있어
   배제.)

3. **커맨드는 core:default로 충분**: 프론트는 우리 커맨드 `open_external_url`
   (core invoke)만 부르고, opener는 Rust 내부에서만 호출하므로 캐패빌리티
   (`opener:allow-open-url`)를 추가하지 않는다. 플러그인 JS API는 노출하지
   않는다(공격 표면 최소화).

4. **Ctrl/Cmd 제스처 필수(프론트)**: web-links addon 핸들러가 우리 핸들러로
   교체되어(기본 `window.open` 대체) **모든 열기가 검증된 백엔드 경로로만**
   흐른다. 열기는 `event.ctrlKey || event.metaKey`일 때만 — 평클릭은 텍스트
   선택 그대로. 게이트 판정은 순수 함수 `shouldActivateLink(ev, enabled)`로
   빼서 addon과 autotest가 공유한다.

5. **플레인 텍스트 URL만**: OSC 8 하이퍼링크(보이는 텍스트 ≠ 실제 URL, 기만
   가능)는 이번 범위 밖. web-links 정규식은 보이는 URL 자체를 링크화하므로
   표시와 대상이 일치한다.

6. **opt-in 토글, 기본 ON**: `UiPrefs.openUrlOnClick`(기본 true). 명시적 Ctrl
   제스처 + 스킴 검증이 있어 기본 ON이 안전하다. 팔레트
   "Links: Enable/Disable Ctrl+click to open URLs"로 끌 수 있다. `ui`는 백엔드
   불투명 JSON이라 schemaVersion(7) 변경·마이그레이션 없음.

## 트레이드오프

- addon은 http/https 외에 `www.`로 시작하는 것도 정규식에 잡을 수 있으나,
  백엔드가 스킴 없는 URL을 거부하므로 그런 클릭은 조용히 무시된다(안전 측 실패).
- 토글이 OFF여도 addon은 로드되어 hover 밑줄은 보인다(열기만 차단). 표시-only
  이슈로 무해.
- 실제 클릭→브라우저 실행은 합성 이벤트로 검증 불가(프로젝트 철학, ADR-010)
  → autotest는 **모디파이어 게이트(순수)** 와 **백엔드 거부 경로**만 검증하고,
  실열기/평클릭 선택은 수동 검증 항목.

## 테스트

- Rust 단위: `is_safe_external_url` 허용(http/https·쿼리 &·포트·프래그먼트·
  대문자 스킴·punycode) / 거부(빈값·javascript·file·ftp·data·스킴상대·공백·
  개행·제어문자·초과길이).
- autotest: `urlOpenGate`(shouldActivateLink 표), `urlRejectsUnsafe`
  (`open_external_url`가 javascript:/file: 거부 — 브라우저 안 띄움).
- 수동: `https://example.com/?a=1&b=2` 출력 → Ctrl+클릭 시 그 URL이 쿼리 보존해
  열림, 평클릭은 선택, 토글 OFF 시 안 열림.

## 향후

- OSC 8 하이퍼링크 지원 시 "표시 텍스트 ≠ 대상 URL"을 사용자에게 보여주는
  확인 UX 필요.
- 링크 hover 시 상태줄에 대상 URL 미리보기(기만 방지).
