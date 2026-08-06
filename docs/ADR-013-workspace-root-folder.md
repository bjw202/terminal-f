# ADR-013: 워크스페이스 시작 폴더 (root folder)

상태: 채택 (SPEC-WORKSPACE-ROOT-001, 2026-08-06)

## 맥락

지금까지 모든 pane은 새 셸을 띄울 때 `model.rs::default_cwd()`가 돌려주는
경로(`%USERPROFILE%` → `$HOME` → `"."`)에서 시작했다. 사용자가 특정 프로젝트
폴더에서 작업하려면 워크스페이스를 열 때마다 매번 `cd`를 입력해야 했다.
워크스페이스는 이미 "작업 단위"를 표현하는 개념인데, 그 작업 단위가 어느
폴더에 속하는지를 표현할 방법이 없었다.

`session.rs::spawn_session`은 이미 `cwd: &str` 인자를 받아 `CommandBuilder::cwd()`에
넘기고 있었다 — "pane을 특정 폴더에서 연다"는 배관(plumbing) 자체는 이미
존재했다. 실제로 새로 필요했던 것은 세 가지뿐이다: (1) `Workspace`에 선택적
`root_dir` 필드, (2) 설정 시 해당 워크스페이스 트리의 모든 `PaneLeaf.cwd`를
다시 쓰는 커맨드, (3) 네이티브 폴더 대화상자 커맨드 + 우클릭 메뉴 항목.

## 결정

1. **데이터 모델**: `Workspace.root_dir: Option<String>`을 `color` 필드
   바로 뒤에 추가한다(JSON 키 `rootDir`, camelCase). `#[serde(default)]`로
   구버전 설정 파일과 호환하고, `skip_serializing_if = "Option::is_none"`으로
   미설정 워크스페이스는 키 자체를 쓰지 않는다.

2. **경로 정규화는 두 계층으로 분리**: 순수 문자열 규칙
   (`normalize_root_dir_str` — 후행 구분자 제거, UNC 가드, trim, 파일시스템
   접근 없음)과 존재 검증(`normalize_root_dir` — `is_dir()` 확인)을 별도
   함수로 나눈다. 단일 함수였다면 UNC 가드나 후행 구분자 규칙 자체를
   검증하는 테스트를 결정적으로 작성할 수 없었다(이 머신에 실재하는 UNC
   공유가 없으므로 항상 `Err`로 떨어져 명세대로는 통과가 구조적으로
   불가능했다). `fs::canonicalize`는 호출하지 않는다 — Windows에서
   `\\?\` 확장 길이 접두사가 붙어 UI 표시와 `CommandBuilder::cwd`로
   그대로 새어 나가기 때문이다.

3. **네이티브 폴더 대화상자 — `rfd` 크레이트, `tauri-plugin-dialog` 아님**.
   `pick_folder` 커맨드를 자체 Rust 커맨드로 구현하고 `rfd`(실측 채택
   버전 0.17.2 — plan 단계의 `"0.16"`은 미검증 추정치였다)를 사용한다.
   기각한 대안은 **`tauri-plugin-dialog`**이며, 근거는 네 가지다:
   - **하우스 스타일**: 이 코드베이스는 모든 OS 통합(클립보드 `arboard`,
     이미지 붙여넣기 `png`, 파이프 `interprocess`, 외부 링크 열기
     `open_external_url`)을 자체 Rust 커맨드로 구현해 왔다. JS 플러그인을
     쓰면 이 선례에서 유일한 예외가 된다.
   - **권한 표면 확장 회피**: Tauri v2에서 앱 자체 커맨드는 permission
     gate를 거치지 않으므로 `src-tauri/capabilities/default.json`은
     `["core:default"]` 그대로 유지된다. `tauri-plugin-dialog` 경로는
     `dialog:allow-open` permission identifier를 새로 추가해야 하며,
     이는 앱이 원하지 않는 범용 파일 대화상자 권한을 webview에 부여하는
     되돌리기 어려운 보안 표면 확장이다.
   - **npm 변경 회피**: 플러그인 경로는 `npm i @tauri-apps/plugin-dialog`와
     `package-lock.json` 변경을 수반한다. `rfd` 경로는 `Cargo.toml`만
     바뀐다.
   - **서버 측 검증 유지**: 선택된 경로가 JS를 왕복하기 전에 백엔드에서
     검증된다(존재 확인 → `set_workspace_root` 왕복).

4. **컨트롤 API 경로 비노출**: `WorkspaceMeta`에 `root_dir`을 추가하되,
   `handle_pipe_method`의 `listWorkspaces` arm은 직렬화 전에 이를 제거한
   payload를 만든다. 인증된 외부 브로커가 사용자 절대 경로를 opt-in 없이
   읽지 못하게 하는 기본 차단(default-deny) 자세(`model.rs`의
   `allow_observe` 주석 참조)를 그대로 지킨다.

5. **스키마 마이그레이션**: `CONFIG_SCHEMA_VERSION`을 7 → 8로 올리면서
   `config.rs::migrate()`의 레거시 arm을 `1 | 2 | ... | 6` 문자 그대로에서
   `1..=7` 범위 형태로 확장한다. 이 한 줄을 빠뜨리면 기존 v7 설정 파일이
   `other =>` 오류 분기로 떨어지고 `lib.rs`가 파일을 `.invalid` 사본으로
   옮긴 뒤 기본 상태로 기동해, 첫 변경 시점에 사용자 워크스페이스가 전량
   덮여 쓰인다 — 이 SPEC에서 가장 위험도가 높은 함정이었다.

6. **로드맵 연결**: `DEVELOPMENT.md` §10(line 236)은 이미 "템플릿 UX: 폴더
   피커 다이얼로그, 템플릿 편집 UI, `${repo}` 자동 일반화"를 확장 후보로
   올려두고 있었다. 본 ADR은 그 로드맵 항목의 첫 조각(폴더 피커)을
   인도한다. 템플릿 편집 UI와 `${repo}` 자동 일반화는 여전히 범위 밖이다.

## 트레이드오프

- `set_root_dir`이 워크스페이스 트리의 **모든** `PaneLeaf.cwd`를 일괄
  재작성하지만, 살아 있는 PTY 세션은 건드리지 않는다 — 실행 중인 셸의
  작업 디렉터리는 외부에서 옮길 신뢰할 수 있는 방법이 없기 때문이다.
  결과적으로 새 시작 폴더는 **재시작 후에만** 적용되며, UI 토스트가 이
  사실을 명시한다.
- `split_pane`의 라이브 cwd 상속(ADR-011)은 시작 폴더로 재정의하지
  않는다 — 사용자가 셸에서 `cd`한 뒤 분할하면 새 pane은 여전히 현재 작업
  위치에서 열리는 것이 의도된 동작이다.
- 루트가 설정된 워크스페이스에서 "Save as template"을 실행하면
  `workspace_as_template`의 기존 "cwds are kept literally" 계약에 따라
  그 절대 경로가 고정된 이식 불가 템플릿이 만들어진다. 이는 본 ADR이
  도입한 결함이 아니라 기존 계약의 귀결이며, `${repo}` 자동 일반화(§로드맵
  연결)가 나오면 해소될 여지가 있다.
- 시작 폴더가 설정된 뒤 그 폴더가 사라지면, `session.rs`의 기존 조용한
  폴백(`if cwd_path.is_dir() { cmd.cwd(cwd_path); }`)이 앱의 cwd를
  상속시킨다 — 크래시는 없지만 팬이 엉뚱한 곳에서 열린다. 이번 SPEC은
  `ensure_sessions`의 기존 `warnings` 벡터로 이 상태를 노출했을 뿐, 사이드바에
  시각적 stale 표시는 추가하지 않았다(프론트가 파일시스템을 stat 할 수
  없어 매 렌더 백엔드 왕복이 필요하기 때문 — 잔여 위험으로 기록).
- **재설계 경위**: `rfd::set_parent(&window)`가 `tauri`와 다른
  `raw-window-handle` 메이저 버전을 쓰면 트레이트 불일치로 빌드가 깨질
  위험이 사전에 식별되었으나, 실제 착수 시점(rfd 0.17.2 + tauri 2.11)에는
  두 크레이트가 같은 메이저를 공유해 컴파일이 그대로 성공했다 — `set_parent`
  제거는 필요하지 않았다.

## 테스트

- Rust 단위: `normalize_str_*`(순수 정규화 — 후행 구분자, UNC 가드, 드라이브
  루트, 공백), `normalize_validated_*`(존재 검증 — 존재/미존재/파일경로/
  None), `set_root_dir_*`(재작성/거부/해제), `v7_fixture_migrates_to_v8`,
  `root_dir_survives_save_load_roundtrip`, `pipe_list_workspaces_omits_root_dir`.
- autotest(headless E2E): `rootDirSet`/`rootDirRewritesPanes`/
  `rootDirRejectsMissing`/`rootDirCleared` — `pick_folder`는 네이티브 모달이라
  호출하지 않고 `set_workspace_root`를 직접 호출해 `C:\Windows`로 검증한다.
- 수동 E2E: 사이드바 우클릭 → "Choose folder…" → 선택 → 앱 완전 종료 후
  재실행 → 해당 워크스페이스의 모든 터미널이 선택한 폴더에서 열림. 대화상자
  취소 시 무변화.

관련 ADR: [ADR-005](ADR-005-live-pty-memory-policy.md)(워크스페이스 상한),
[ADR-011](ADR-011-shell-integration-live-cwd.md)(라이브 cwd 상속 — 본 SPEC은
이를 재정의하지 않는다), [ADR-012](ADR-012-url-open-security.md)(외부 열기
보안 — 같은 "자체 Rust 커맨드" 하우스 스타일 선례).
