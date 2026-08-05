---
id: SPEC-WORKSPACE-ROOT-001
title: "워크스페이스 시작 폴더 — 구현 계획"
version: "0.4.0"
status: in-progress
created: 2026-08-05
updated: 2026-08-05
author: manager-spec
priority: P1
phase: "v0.1.2 target"
module: "src-tauri/src, src"
lifecycle: spec-anchored
tags: "workspace, config, persistence, tauri, pty, dialog"
tier: M
---

# SPEC-WORKSPACE-ROOT-001 — 구현 계획

> 이 문서는 **되돌리기 어려운 결정을 먼저** 배치한다. §A.1~§A.3(데이터 모델·대화상자 방식·UX 흐름)이 가장 바뀔 가능성이 높은 결정이고, §F 마일스톤 후반부는 기계적인 배선 작업이다. 리뷰는 앞쪽에 집중하면 된다.

---

## §A 배경과 핵심 설계 결정

### A.1 데이터 모델 결정 — `Workspace.root_dir: Option<String>`

가장 되돌리기 어려운 결정이다. 한 번 `config.json`에 기록되기 시작하면 형태 변경에는 또 한 번의 스키마 마이그레이션이 필요하다.

```rust
// model.rs — Workspace 구조체, `color` 필드 바로 뒤에 배치
/// 워크스페이스 시작 폴더. `root`(팬 트리)와 혼동하지 말 것.
/// 설정되면 이 워크스페이스의 모든 팬이 이 폴더에서 셸을 연다.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub root_dir: Option<String>,     // JSON 키: "rootDir"
```

- `Workspace`에는 이미 `#[serde(rename_all = "camelCase")]`가 걸려 있으므로 JSON 키는 자동으로 `rootDir`이 된다.
- `#[serde(default)]` — 구버전 설정 파일에서 필드가 없어도 파싱된다. 이것이 §B.1 마이그레이션 전략의 전제다.
- `skip_serializing_if = "Option::is_none"` — 미설정 워크스페이스는 `rootDir` 키 자체를 쓰지 않는다. 기존 설정 파일과의 디프를 최소화한다.
- **배치 위치 주의**: `pub root: PaneNode`(팬 트리)에서 두 줄 떨어진 곳에 `root_dir`이 놓인다. 이름이 비슷하므로 doc comment를 반드시 붙인다.

경로 정책은 store가 아니라 `model.rs`의 함수로 분리한다. 나아가 **문자열 정규화와 존재 검증을 두 함수로 나눈다**.

```rust
// model.rs — (1) 순수 함수: 파일시스템에 접근하지 않는다
pub fn normalize_root_dir_str(raw: &str) -> Option<String>

// model.rs — (2) 검증 포함: (1) + is_dir() 확인
pub fn normalize_root_dir(raw: Option<String>) -> Result<Option<String>, String>
```

**(1) `normalize_root_dir_str` — 순수 문자열 규칙**
1. `trim()`, 빈 문자열이면 `None`
2. 후행 `\` 또는 `/`를 **한 개만** 제거. 단 남은 길이가 3자 미만(`C:\`)이거나 경로가 `\\`로 시작(UNC 공유 루트)하면 제거하지 않는다
3. 파일시스템을 건드리지 않는다 → 어떤 경로 리터럴로도 결정적으로 테스트 가능

**(2) `normalize_root_dir` — 존재 검증 계층**
1. `raw`를 (1)에 위임. `None`이면 `Ok(None)`
2. `Some(p)`이면 `Path::new(&p).is_dir()`가 참이어야 하며, 거짓이면 `Err("not an existing folder: ...")`
3. **`fs::canonicalize`를 호출하지 않는다.** Windows에서 `\\?\` 확장 길이 접두사를 붙여 반환하며, 이 값이 UI 표시와 `CommandBuilder::cwd`로 그대로 새어 나간다

`set_root_dir`은 (2)를 호출한다 — store 계층의 계약은 분리 이전과 동일하다.

**왜 나누는가 (plan-audit D2/D11 대응)**: 단일 함수였을 때 `is_dir()` 게이트 때문에 `C:\work\` → `C:\work`, `\\host\share\` → `\\host\share\` 같은 **정규화 규칙 자체를 검증하는 케이스를 작성할 수 없었다**. 이 머신에 `C:\work`도 실재하는 UNC 공유도 없으므로 두 케이스는 항상 `Err`로 떨어진다. 특히 UNC 가드는 실재하는 네트워크 공유 없이는 어떤 방법으로도 검증할 수 없어, 가드를 삭제해도 통과하는 공허한 테스트가 된다. 순수 함수 분리는 이 두 문제를 동시에 해결하는 유일한 방법이다. 존재 검증은 `C:\Windows`(autotest가 이미 "guaranteed to exist"로 쓰는 경로)와 temp-dir로 별도 검증한다.

### A.2 대화상자 방식 결정 — `rfd` 크레이트 (Rust 커맨드), `tauri-plugin-dialog` 아님

**채택: `rfd` 크레이트 + 자체 `pick_folder` 커맨드** (버전은 M3 착수 시 `cargo add rfd`로 확정한다 — `"0.16"`은 미검증 후보값이며, `Cargo.toml`에 `rfd`가 없어 오프라인 판정이 불가능하다)

근거 네 가지:

1. **하우스 스타일** — 이 코드베이스는 모든 OS 통합을 자체 Rust 커맨드로 구현해 왔다. 클립보드는 `arboard`, 이미지 붙여넣기는 `png`, 파이프는 `interprocess`, 외부 링크는 `open_external_url`. `pick_folder` 커맨드는 이 선례를 그대로 따른다. `sidebar.ts`에서 JS 플러그인을 호출하면 코드베이스에서 유일한 예외가 된다.
2. **권한 표면 없음** — Tauri v2에서 앱 자체 커맨드는 permission gate를 거치지 않으므로 `src-tauri/capabilities/default.json`은 `["core:default"]` 그대로 유지된다. 플러그인 경로는 `dialog:allow-open` permission identifier를 추가해야 하며, 이는 앱이 원하지 않는 범용 파일 대화상자 권한을 webview에 부여한다.
3. **npm 변경 없음** — 플러그인 경로는 `npm i @tauri-apps/plugin-dialog`와 `package-lock.json` 변경을 수반한다.
4. **검증이 서버 측에 머문다** — 선택된 경로가 JS를 왕복하기 전에 백엔드에서 검증된다.

**기각된 대안: `tauri-plugin-dialog`** — 위 4개 항목의 반대급부를 감수해야 한다. 특히 `capabilities/default.json`에 `dialog:allow-open`을 추가하는 것이 되돌리기 어려운 보안 표면 확장이다. 향후 독자가 "왜 플러그인을 안 썼지"를 재검토할 때 이 기각 근거를 참조한다.

### A.3 UX 흐름 결정 — 기존 컬러 메뉴를 확장, 서브메뉴 아님

`sidebar.ts`의 `openColorMenu()`가 만드는 `#sb-colormenu` div를 **평평한 두 번째 섹션**으로 확장한다. 서브메뉴로 만들지 않는다.

- 기존 바깥 `mousedown`(capture 단계) 닫기 핸들러와 `sidebarBusy` 가드가 평평한 메뉴에서는 그대로 동작한다. 서브메뉴는 두 핸들러 모두를 다시 설계해야 한다.
- 메뉴 구성: 기존 컬러 스와치 행 → 구분선 → "Root folder" 라벨 → 현재 경로 줄(`title`에 전체 경로) 또는 "Not set" → "Choose folder…" 버튼 → 루트가 설정된 경우에만 "Clear" 버튼
- **호출 순서 불변식**: 콜백 **이전에** `close()`를 호출한다. 기존 스와치 핸들러와 동일한 순서다. 그러면 네이티브 대화상자가 열려 있는 동안 `sidebarBusy`가 이미 `false`이므로, 1초 주기 activity poll의 재렌더가 무해하다.
- 식별자 이름 변경: `openColorMenu` → `openWorkspaceMenu`, DOM id `sb-colormenu` → `sb-wsmenu`. 더 이상 색상 전용이 아니기 때문이다. TS 참조 4곳 + CSS 선택자 3곳이며 다른 참조는 없다. `.sb-colormenu-*` / `.sb-swatch` **클래스 이름은 건드리지 않는다** — 디프 최소화.

### A.4 `WorkspaceMeta`에 `root_dir`을 추가하는 이유 — 그리고 그 두 번째 소비자

`metas()`가 돌려주는 `WorkspaceMeta { id, name, color }`는 프론트엔드가 보유하는 **유일한** 워크스페이스 목록이다. 전체 `Workspace`는 활성 워크스페이스에 대해서만 존재한다. 컨텍스트 메뉴는 **비활성 워크스페이스를 포함한 모든** 우클릭 대상에 대해 현재 경로를 렌더링하고 "Clear" 표시 여부를 결정해야 한다(R11). 따라서 `WorkspaceMeta`에 `root_dir`이 필요하다.

#### A.4.1 `metas()`의 소비자는 프론트엔드만이 아니다 (plan-audit D3)

`grep -n 'metas' src-tauri/src/commands.rs`로 호출자를 전수 열거하면 두 번째 소비자가 드러난다. `commands.rs` `handle_pipe_method`:

```rust
"listWorkspaces" => {
    let store = state.store.lock().unwrap();
    Ok(serde_json::to_value(store.metas()).unwrap_or_default())
}
```

`metas()`를 **그대로 직렬화**한다. 필드를 추가하는 순간 인증된 외부 브로커가 사용자의 절대 파일시스템 경로를 **아무 opt-in 없이** 읽는다.

이것이 왜 그냥 넘길 수 없는 항목인가:

- `model.rs:42-44`가 `allow_observe`를 기본 `false`로 두는 이유를 명문화한다 — "terminal contents (which may hold secrets) are not exposed to external brokers unless enabled".
- `docs/DEVELOPMENT.md:207` 불변식 3: **게이트 우회 금지**.
- 워크스페이스 루트 경로는 터미널 출력만큼 민감하지는 않으나 **새로운 외부 노출 표면**인 것은 분명하다. 이 코드베이스의 기본 차단(default-deny) 자세상 의식적 결정 없이 통과시켜서는 안 된다.

#### A.4.2 결정 — 프론트엔드에는 싣고, 파이프 경계에서 벗긴다

**채택**: `WorkspaceMeta`에 `root_dir`을 유지하되, `handle_pipe_method`의 `listWorkspaces` arm이 직렬화 전에 이를 제거한 payload를 만든다. 브로커 표면은 오늘과 **정확히 동일**하게 유지된다.

근거: 이 프로젝트의 기본 차단 자세와 "파이프는 게이트를 우회할 수 없다"는 강제 지점(`commands.rs`)을 그대로 보존한다. 비용은 약 3줄이다. `listWorkspaces`는 `metas()`의 유일한 파이프 소비자이므로 국소적이다.

기각한 대안:

| 대안 | 기각 사유 |
|---|---|
| 프론트엔드 전용 DTO를 신설하고 `WorkspaceMeta`는 그대로 둠 | 타입이 하나 늘고 `metas()`가 둘로 갈라진다. `get_state`/`AppSnapshot`(`commands.rs:139-146`)도 `metas()`를 쓰므로 그쪽까지 분기해야 한다. 파이프 arm 3줄보다 비싸다 |
| 노출을 허용하고 §E에 비목표로 기록 | 기본 차단 자세를 되돌린다. 노출을 원하는 소비자가 현재 하나도 없는데 표면만 넓힌다 |
| `allow_observe`류 opt-in 게이트 신설 | 명백한 범위 확대. 설정 스키마·UI·문서가 모두 딸려온다. 요구하는 사용자가 없다 |

비용 정리: 워크스페이스당 `Option<String>` 하나(hard cap 16개로 상한) + 파이프 arm 약 3줄. 검증은 acceptance.md **AC-12**, 종료 게이트 항목은 §D.4.1.

### A.5 `new_workspace` 이음새 (seam)

`new_workspace(name)`은 자유 함수라 store 컨텍스트가 없어 루트를 알 수 없다. 생성 흐름은 변경하지 않으므로(spec.md §E) `new_workspace(name)`은 **그대로 둔다**. 대신 이음새만 만들어 둔다:

```rust
pub fn new_workspace_in(name: &str, cwd: &str, root_dir: Option<String>) -> Workspace
```

호출자(루트를 아는 쪽)가 cwd를 공급하는 형태다. 오늘 호출 지점 변경은 없다. 이 이음새의 목적은, 훗날 누군가가 store 참조를 자유 함수까지 관통시키는 설계를 하지 않도록 막는 것이다.

> **"그대로 둔다"의 정확한 범위 (plan-audit D4)**: `new_workspace(name)`의 **시그니처와 호출 지점**을 바꾸지 않는다는 뜻이다. **함수 본문은 수정해야 한다** — `state.rs:255`의 `Workspace { … color: None, … }` struct 리터럴에 `root_dir: None`을 추가해야 컴파일된다. Rust의 `#[serde(default)]`는 **역직렬화에만** 작용하며 struct 리터럴에는 아무 영향이 없다. "손댈 필요 없다"로 읽지 말 것.

---

## §B 알려진 문제와 위험 (Known Issues)

### B.1 [최고 위험] 스키마 버전 범프가 기존 설정을 폐기하는 함정

**확인된 사실** (직접 읽어 검증함):

- `src-tauri/src/model.rs:14` — `pub const CONFIG_SCHEMA_VERSION: u32 = 7;`
- `src-tauri/src/config.rs:24-37` — `migrate()`의 match 구문. 레거시 arm이 **문자 그대로** `1 | 2 | 3 | 4 | 5 | 6`(line 28)이며, 그 외 버전은 `other => Err(...)`(line 34-36)로 떨어진다

```rust
// config.rs:24-37 (현재 코드)
match version {
    CONFIG_SCHEMA_VERSION => { ... }
    1 | 2 | 3 | 4 | 5 | 6 => { ...; cfg.schema_version = CONFIG_SCHEMA_VERSION; ... }
    other => Err(format!(
        "unsupported config schemaVersion {other} (supported: 1..={CONFIG_SCHEMA_VERSION})"
    )),
}
```

**함정**: `CONFIG_SCHEMA_VERSION`을 7 → 8로 올리면서 레거시 arm을 확장하지 않으면, 기존 v7 설정 파일이 전부 `other =>` 오류 분기로 떨어진다. 그 다음에 일어나는 일은 `lib.rs:43-54`에 있다:

```rust
Err(e) => {
    // Corrupt/unsupported config: keep the file for inspection,
    // start with a fresh default rather than crashing.
    eprintln!("[terminal-f] config load failed, starting fresh: {e}");
    let backup = config_path.with_extension("json.invalid");
    let _ = std::fs::copy(&config_path, backup);   // rename이 아니라 copy
    WorkspaceStore::with_default().to_config(Vec::new())
}
```

정확한 메커니즘 (plan-audit D1로 정정):

1. `fs::rename`이 **아니라 `fs::copy`**다. 주석이 명시적으로 "keep the file for inspection"이라고 말하며, 원본 `config.json`은 그 자리에 **남는다**.
2. 앱은 기본 상태(`WorkspaceStore::with_default()`)로 기동한다.
3. 사용자가 무엇이든 변경하는 **첫 `persist()` 시점에 `save_config`가 `config.json`을 기본 상태로 덮어쓴다.** 이때 원본 내용이 사라진다.

따라서 결과를 정직하게 표현하면: **사용자는 앱을 열자마자 워크스페이스가 전부 사라진 화면을 보고, 첫 변경과 함께 `config.json`이 기본값으로 덮어써진다.** 다만 `config.json.invalid` 사본이 남아 있으므로 **수동 복구는 가능하다** — "복구 불가능한 전량 소실"은 아니다. 그럼에도 일반 사용자가 `%APPDATA%`를 뒤져 `.invalid` 파일을 되살릴 것으로 기대할 수는 없으므로, 실질적 피해 등급은 여전히 본 SPEC 최고 수준이다.

**수정**: line 28을 범위 형태로 바꾼다.

```rust
1..=7 => { ... }
```

이후의 버전 범프는 한 글자 편집으로 끝난다.

**가드**: 회귀 테스트 `v7_fixture_migrates_to_v8`가 M1의 **최초 RED 테스트**여야 한다. 다른 어떤 코드보다 먼저 작성한다.

이 함정은 acceptance.md AC-1에도 동일하게 기록되어 있다. 본 SPEC에서 놓치기 가장 쉽고, 놓쳤을 때 결과가 가장 나쁜 항목이다.

### B.2 [중간 위험] `spawn_session`의 조용한 폴백

**확인된 사실**: `src-tauri/src/session.rs:462-466`

```rust
let mut cmd = CommandBuilder::new(&program);
let cwd_path = Path::new(cwd);
if cwd_path.is_dir() {
    cmd.cwd(cwd_path);
}
```

`cwd`가 존재하는 디렉터리가 아니면 `cmd.cwd()` 호출 자체를 건너뛰고 자식 프로세스가 앱의 cwd를 상속한다. **오류도 없고 사용자에게 보이는 신호도 없다.** 시작 폴더를 설정한 뒤 그 폴더를 삭제하면, 팬은 열리지만 엉뚱한 곳에서 열린다. M5에서 `ensure_sessions`의 기존 `warnings` 벡터에 항목을 넣어 이 폴백을 노출한다(`main.ts:59`에서 이미 표시 경로가 존재하므로 새 UI가 필요 없다).

### B.3 [중간 위험] 네이티브 대화상자의 이벤트 루프 블로킹

`src/modal.ts` 헤더는 WebView2 대화상자가 이벤트 루프를 막기 때문에 네이티브 `window.prompt`를 피한다고 명시한다. 동일한 위험이 `pick_folder`에도 적용된다. `#[tauri::command(async)]`를 **동기 함수에 붙여** 블로킹 본문을 UI 스레드 밖으로 옮긴다. 이 어트리뷰트를 빠뜨리면 대화상자가 WebView2 이벤트 루프를 교착시킬 수 있다.

두 가지를 명시해 둔다:

- **비용**: 이 방식은 대화상자가 열려 있는 동안(사용자가 수 분간 폴더를 고를 수 있다) async 런타임 워커 스레드 1개를 점유한다. `pick_folder`는 동시에 여러 번 열릴 수 없는 UI이므로 실질 영향은 스레드 1개이며 허용 범위다.
- **미평가 대안**: `rfd`가 제공하는 `AsyncFileDialog`(진짜 비동기 API)는 평가하지 않았다. 워커 스레드 점유가 문제가 되면 M3에서 이 대안으로 전환할 수 있다.

### B.4 [낮은 위험] `rfd::set_parent`의 raw-window-handle 트레이트 불일치

`.set_parent(&window)`는 대화상자를 앱 창 소유로 유지한다(대화상자가 창 뒤로 숨지 않게 한다). `rfd`와 `tauri`가 서로 다른 `raw-window-handle` 메이저 버전을 쓰면 빌드 시 트레이트 불일치가 발생할 수 있다. 그 경우 `set_parent`를 제거하는 것이 허용 가능한 성능 저하(graceful degradation)다. 기능은 동작하며 창 소유 관계만 잃는다.

---

## §C 사전 점검 (Pre-flight)

M1 시작 전에 확인한다. 세 항목은 이미 검증되었다(2026-08-05, 직접 읽음).

| # | 확인 대상 | 결과 |
|---|---|---|
| 1 | `config.rs`의 레거시 arm이 `1 \| 2 \| 3 \| 4 \| 5 \| 6`이고 `CONFIG_SCHEMA_VERSION == 7` | 확인 (config.rs:28, model.rs:14) |
| 2 | `layout::collect_panes_mut`가 존재하고 `Vec<&mut PaneLeaf>` 반환 | 확인 (layout.rs:193-206) |
| 3 | `session.rs`의 `if cwd_path.is_dir() { cmd.cwd(cwd_path); }` 조용한 폴백 | 확인 (session.rs:463-466) |

추가 확인 대상:

- `cargo test` 기준선이 green인지 (M1 RED 테스트가 유일한 실패여야 함)
- `state.rs::set_color`(state.rs:166-173)의 형태 — `set_root_dir`은 이 바로 뒤에 배치하여 대칭을 이룬다
- **`Workspace { … }` struct 리터럴 3곳 전부** — 필드를 추가하면 세 곳 모두 컴파일 오류가 난다. `grep -rn 'color: None' src-tauri/src/`로 전수 확인한 결과(plan-audit D4):

| # | 위치 | 함수 | 비고 |
|---|---|---|---|
| 1 | `state.rs:203` | `create_with_root` (state.rs:177-207) | 템플릿 적용 경로 |
| 2 | `state.rs:255` | `new_workspace` (state.rs:244-257) | §A.5의 "그대로 둔다"는 시그니처 한정 — 리터럴은 수정 필요 |
| 3 | `config.rs:123` | `sample_config()` 테스트 헬퍼 (config.rs:113-128) | M1에서 마이그레이션 테스트를 추가하는 바로 그 파일 |

  컴파일러가 즉시 잡아주므로 치명적이지는 않으나, 사전 점검의 목적은 이런 중단을 없애는 것이다. 특히 2번은 §A.5가 한때 "건드리지 않는다"로 오독될 여지를 준 지점이다

---

## §D 제약 (Constraints)

- **TDD**: 각 마일스톤은 RED → GREEN → REFACTOR 사이클. 테스트를 먼저 쓰고 실패를 확인한 뒤 구현한다.
- **커밋당 최소 커버리지 80%, 목표 85%** (`quality.yaml`).
- **`capabilities/default.json` 불변**: `["core:default"]` 유지.
- **`fs::canonicalize` 금지**: §A.1 4항.
- **락 순서**: store → registry. 역전 금지. `set_workspace_root`는 store 락을 잡고 트리 순회를 마친 뒤 락을 **놓고** `persist(&state)`를 호출한다. `set_workspace_color`와 동일한 규율이다.
- **`ensure_sessions` 호출 금지**: `set_workspace_root`는 세션을 스폰하지 않는다. 띄울 것이 없고, 기존 세션은 의도적으로 옛 cwd를 유지한다.
- **단계 분리**: 소스 코드는 run 단계(M1~M5)에서, 문서는 sync 단계(§F.S S1~S7 — ADR-013 신규 + 동반 5종 + ARCHITECTURE)에서. M5에는 문서 작업이 포함되지 않는다.
- **Route A (Hybrid Trunk)**: Tier M이므로 PR 없이 `main`에 직접 커밋·푸시.

---

## §E 자체 검증 (Self-Verification)

각 마일스톤 완료 시 아래를 실행하고 verbatim 출력을 근거로 남긴다.

```bash
cd src-tauri && cargo test                     # 전체 Rust 테스트
cd src-tauri && cargo clippy -- -D warnings    # 신규 경고 0
npm run build                                  # 프론트엔드 빌드 (M4 이후)
```

M1 완료 시점에 `cargo test`가 green이어야 M2로 넘어간다. M1만으로 완전한 하위 호환이 성립한다.

---

## §F 마일스톤

각 마일스톤은 하나의 RED → GREEN → REFACTOR 사이클이다.

### M1 — 모델 + 마이그레이션 + store 메서드 (가장 위험, 가장 먼저)

**RED (이 순서를 지킬 것)**
1. `config.rs` 테스트 모듈에 `v7_fixture_migrates_to_v8` 추가 — 현실적인 v7 픽스처가 `schema_version == 8`, `root_dir.is_none()`을 만족해야 함. **지금은 실패한다.**
2. `root_dir_survives_save_load_roundtrip`
3. `state.rs` 테스트 모듈에 `set_root_dir_rewrites_leaf_cwds`, `set_root_dir_rejects_missing_folder`, `set_root_dir_rejects_file_path`, `set_root_dir_unknown_workspace_errors`, `set_root_dir_blank_clears`, `set_root_dir_clear_keeps_existing_cwds`
4. `model.rs` 테스트 모듈에 **순수 함수** 케이스 — 파일시스템 무관하므로 어떤 리터럴이든 결정적으로 검증된다 (`C:\work\` → `C:\work`, `C:\` 불변, `\\host\share\` 불변, `"  "` → `None`)
5. `model.rs` 테스트 모듈에 **검증 계층** 케이스 — `C:\Windows\` → `Ok(Some("C:\Windows"))`, 존재하지 않는 경로 → `Err`, temp-dir 생성 경로 → `Ok`

> **[HARD] 테스트 이름 규약 — 4번과 5번은 접두사를 달리한다**
>
> | AC | 접두사 | 예시 | 게이트 명령 |
> |---|---|---|---|
> | AC-10a (순수) | `normalize_str_` | `normalize_str_strips_one_trailing_separator`, `normalize_str_keeps_drive_root`, `normalize_str_keeps_unc_prefix`, `normalize_str_blank_is_none` | `cargo test normalize_str_` |
> | AC-10b (검증) | `normalize_validated_` | `normalize_validated_accepts_existing_dir`, `normalize_validated_rejects_missing`, `normalize_validated_rejects_file_path`, `normalize_validated_none_is_ok_none` | `cargo test normalize_validated_` |
>
> `cargo test <필터>`는 **부분 문자열 매칭**이다. 함수명을 그대로 딴 이름(`normalize_root_dir_…` / `normalize_root_dir_str_…`)을 쓰면 `cargo test normalize_root_dir`가 양쪽을 모두 매치해, 한쪽 AC의 테스트가 **하나도 없어도 명령이 exit 0으로 통과**한다. §D.4.1이 이 명령으로 게이트하므로 종료 게이트가 그 부재를 탐지하지 못한다. 위 두 접두사는 서로의 부분 문자열이 아니므로 필터가 상호 배타적이며, 비어 있는 쪽은 "0 tests"로 드러난다. 함수 이름(`normalize_root_dir_str` / `normalize_root_dir`)과 테스트 이름은 별개다 — 테스트는 반드시 위 접두사를 쓸 것.

**GREEN**
- `model.rs`: `CONFIG_SCHEMA_VERSION` 7 → 8, `Workspace.root_dir` 필드(§A.1), `normalize_root_dir_str` + `normalize_root_dir` 두 함수(§A.1), `new_workspace_in` 이음새(§A.5)
- **`Workspace { … }` struct 리터럴 3곳 전부**에 `root_dir: None` 추가 — `state.rs:203`, `state.rs:255`, `config.rs:123` (§C 표)
- `config.rs:28`: `1 | 2 | 3 | 4 | 5 | 6` → `1..=7` (**§B.1 — 이 한 줄이 본 SPEC에서 가장 중요하다**)
- `config.rs`: 기존 v1/v2/v3 마이그레이션 테스트에 `root_dir.is_none()` 단언 추가
- `state.rs`: `set_color` 바로 뒤에 `set_root_dir` 추가

```rust
// state.rs — set_color 바로 뒤에 배치 (대칭)
pub fn set_root_dir(&mut self, id: &str, root_dir: Option<String>) -> Result<usize, String> {
    // VALIDATE-BEFORE-MUTATE (reorder와 동일한 계약):
    // normalize를 get_mut보다 먼저 호출해야 잘못된 경로가 절대 반쯤 적용되지 않는다.
    let dir = normalize_root_dir(root_dir)?;                       // 1
    let ws = self.get_mut(id)
        .ok_or_else(|| format!("workspace not found: {id}"))?;     // 2
    ws.root_dir = dir.clone();                                     // 3
    let mut n = 0;
    if let Some(d) = dir {                                         // 4
        for leaf in layout::collect_panes_mut(&mut ws.root) {
            leaf.cwd = d.clone();
            n += 1;
        }
    }
    ws.updated_at = now_ms();                                      // 5
    Ok(n)
}
```

- `create_with_root`는 `root_dir: None`만 추가한다 (템플릿에서 루트를 유도하지 **않는다** — spec.md §E)

**REFACTOR** — `collect_panes_mut`(layout.rs:193) 재사용을 확인한다. 새 트리 순회 함수를 작성하지 않는다.

**완료 조건**: `cargo test` green. 이 마일스톤만으로 완전한 하위 호환이 성립하며, M2 이전에 반드시 green이어야 한다.

### M2 — 커맨드 계층

**RED**
1. `SetWorkspaceRootResult` 직렬화 형태 테스트 (camelCase 키 고정)
2. **`pipe_list_workspaces_omits_root_dir`** — `handle_pipe_method`의 `listWorkspaces` 반환값에 `rootDir` 키가 없음을 단언한다. AC-12의 차단 테스트이며 `§D.4.1` 종료 게이트 항목이므로 RED에서 먼저 작성한다(`development_mode: tdd`). 짝으로 프론트엔드 경로(`metas()` 직접 호출)에는 `root_dir`이 실려 있음을 단언해, "필드를 그냥 빼버린" 회귀와 구분한다

**GREEN**
- `state.rs::metas()`에 `root_dir: w.root_dir.clone()` 추가, `WorkspaceMeta`에 필드 추가 (§A.4)
- `commands.rs`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetWorkspaceRootResult {
    pub workspaces: Vec<WorkspaceMeta>,
    pub workspace: Workspace,
    pub rewritten: usize,
}

#[tauri::command]
pub fn set_workspace_root(
    state: State<'_, AppState>,
    workspace_id: String,
    root_dir: Option<String>,
) -> Result<SetWorkspaceRootResult, String>
```

  흐름: store 락 획득 → `set_root_dir()?` → workspace 복제 + `metas()` → **락 해제** → `persist(&state)` → 반환.
  `ensure_sessions`를 호출하지 **않는다**(§D).
  전체 `workspace`를 반환하는 이유: 프론트엔드가 `main.ts:717`(rule repo default)과 `main.ts:1020`(모달 초기값)에서 `leaf.cwd`를 읽는다. metas만 반환하면 이 두 곳이 stale해진다.

- `lib.rs`의 `invoke_handler`에 `set_workspace_root` 등록
- **파이프 경계에서 `root_dir` 제거** (§A.4.2, spec.md §C, AC-12): `handle_pipe_method`의 `listWorkspaces` arm이 `store.metas()`를 그대로 직렬화하지 않고, `root_dir`을 뺀 payload로 매핑한 뒤 직렬화한다. 브로커 응답은 본 SPEC 이전과 바이트 단위로 동일해야 한다. 약 3줄
- `commands.rs::split_pane`(약 line 293)에 주석 추가: 시작 폴더는 live cwd 상속을 **의도적으로 재정의하지 않는다** (ADR-011)

### M3 — 네이티브 폴더 대화상자

**GREEN**
- `src-tauri/Cargo.toml`에 `rfd` 추가. **버전은 `cargo add rfd`로 실제 최신 안정 버전을 확인해 기입한다** — `"0.16"`은 미검증 추정치이며, `Cargo.toml`에 `rfd`도 `raw-window-handle`도 현재 직접 의존성으로 없어 오프라인 사전 판정이 불가능하다 (§B.4의 트레이트 불일치 위험도 이 시점에 처음 판정된다)
- `commands.rs`:

```rust
#[tauri::command(async)]   // 동기 fn에 (async) — 블로킹 본문을 UI 스레드 밖으로 (§B.3)
pub fn pick_folder(window: tauri::Window, initial: Option<String>) -> Result<Option<String>, String>
```

  `rfd::FileDialog::new().set_title(...)` → `initial`이 `is_dir()`이면 `.set_directory(initial)` → `.set_parent(&window)` → `.pick_folder().map(|p| p.to_string_lossy().into_owned())`.
  `set_parent`가 빌드되지 않으면 제거 허용 (§B.4).

- `lib.rs`의 `invoke_handler`에 `pick_folder` 등록
- `capabilities/default.json`은 **변경하지 않는다** (§A.2 근거 2)

### M4 — 프론트엔드 배선

- `src/types.ts`: `WorkspaceMeta.rootDir: string | null`, `Workspace.rootDir?: string | null`, `SetWorkspaceRootResult`
- `src/ipc.ts`: `setWorkspaceRoot(workspaceId, rootDir)`, `pickFolder(initial?)`
- `src/sidebar.ts`:
  - `SidebarProps`에 `onPickRoot(id)`, `onClearRoot(id)` 추가
  - `openColorMenu` → `openWorkspaceMenu`, `sb-colormenu` → `sb-wsmenu` (§A.3)
  - 평평한 두 번째 섹션 추가 (§A.3 메뉴 구성)
  - `shortPath(p, max = 34)` 헬퍼 — 결정적(deterministic) 축약: 첫 세그먼트 + `…` + 마지막 두 세그먼트. 전체 경로는 `title` 속성
  - **호출 순서 불변식**: 콜백 전에 `close()` (§A.3)
- `src/main.ts`:
  - `pickWorkspaceRoot(id)` — metas에서 현재 rootDir을 읽어 대화상자 시작 디렉터리로 사용. **취소 시 조용히 조기 반환** (IPC·토스트·재렌더 없음, R10)
  - `applyWorkspaceRoot(id, dir)` — `metas = res.workspaces`; `current?.id === id`면 `current = res.workspace`; `refreshSidebar()`; 성공 토스트는 **"재시작 후 적용"을 반드시 명시**(그래야 살아 있는 터미널이 그대로인 것이 버그가 아니라 설명된 동작으로 읽힌다); `catch → showStatus(String(e), true)`로 "not an existing folder" 오류 노출
  - 커맨드 팔레트 항목 2개를 `ws.new`(약 line 511) 옆에 추가:
    - `{ id: "ws.root", title: "Workspace: Set root folder…" }`
    - `{ id: "ws.root.clear", title: "Workspace: Clear root folder" }`
    - 이 두 항목 때문에 `GUIDE-command-palette.md` 갱신 의무가 발동된다 (§F.S 참조)
- `src/styles.css`: `#sb-colormenu` → `#sb-wsmenu`, `min-width: 200px; max-width: 280px` 추가, 구분선 / 경로 줄 / 메뉴 항목 버튼 규칙 추가 (해당 블록이 이미 쓰고 있는 CSS 변수 재사용)

### M5 — 관측성 + E2E (run 단계 꼬리, 가장 기계적)

- `commands.rs::ensure_sessions`(약 line 71): 팬의 `cwd`가 `is_dir()`이 아니면 기존 `warnings` 벡터에 항목 추가 (§B.2). `main.ts:59`에 표시 경로가 이미 있으므로 새 UI 없음
- `src/autotest.ts`: `rootDirSet`, `rootDirRewritesPanes`, `rootDirRejectsMissing`, `rootDirCleared` 체크 추가. **`pick_folder`를 호출하지 않는다** — 네이티브 모달이 headless 실행을 멈춘다. `set_workspace_root`를 직접 호출하고 `C:\\Windows`를 쓴다(해당 파일 line 499/531에서 이미 사용 중인 보장된 경로). 최종 `report.ok` 결합(약 line 601)에 새 체크 추가

> **M5는 run 단계로 끝난다.** 문서 작업은 M5에 포함되지 않는다 — §F.S를 참조한다. 이전 판의 M5는 코드 변경과 문서 변경을 한 제목 아래 섞고 있었고, 이는 실행 주체(manager-develop)와 산출 주체(manager-docs)가 다르다는 사실을 감췄다. 여기서 분리한다.

---

## §F.S 동기화 단계 산출물 (sync phase — `/moai sync`, manager-docs)

run 단계(M1~M5) 완료 **이후** 수행한다. 실행 주체는 manager-docs이며, 검증 기준은 acceptance.md **AC-11**이다.

근거: `docs/DEVELOPMENT.md` §9 「문서 규칙」(line 218-228). 이 프로젝트는 기능마다 ADR 1건과 동반 문서 5종 갱신을 의무화한다. 이전 판의 M5는 이 중 2건(DEVELOPMENT.md, ARCHITECTURE.md)만 담고 있었다.

### S1 — ADR-013 신규 작성 (**새 파일**)

`docs/ADR-013-workspace-root-folder.md` (파일명 후미는 조정 가능, 번호는 **013** 고정 — ADR-001~012가 이미 존재하며 013이 다음 빈 번호다).

- **작성 언어: 한국어** (§9 line 220-222 — "모든 문서는 한국어로 작성한다", 코드 식별자·파일명·명령어는 영어 유지)
- **필수 4개 절 구조** (§9 line 223-224): 배경 → 결정 → 트레이드오프 → 테스트
- **표기 주의 (plan-audit D6)**: 기존 ADR은 표기가 갈린다. 실측 결과 — ADR-010은 `## 배경 / ## 결정 / ## 테스트`(트레이드오프 없음), ADR-011은 `## 맥락 / ## 결정 / ## 트레이드오프`(테스트 없음), ADR-012는 `## 맥락 / ## 결정 / ## 트레이드오프 / ## 테스트`. **네 절을 모두 갖춘 선례는 ADR-012뿐이므로 구조 모델은 ADR-012로 삼는다.** AC-11 11-b는 `배경`과 `맥락`을 동의어로 허용하되 4개 절을 요구한다. ADR-010은 §9가 "실패 후 재설계 경위의 모델"로 지목한 문서이지 구조 모델이 아니다 — 서술 방식만 참고할 것
- **반드시 담을 결정**: `rfd` vs `tauri-plugin-dialog` 선택. 현재 이 비교는 plan.md §A.2에만 있는데, 이 프로젝트는 그런 결정을 ADR에 영구 보관한다. 기각 근거(`dialog:allow-open` permission identifier 추가 회피, 하우스 스타일, npm 변경 회피, 서버 측 검증 유지)를 그대로 옮긴다
- **로드맵 연결**: `DEVELOPMENT.md` §10 line 236이 이미 "템플릿 UX: **폴더 피커 다이얼로그**, 템플릿 편집 UI, `${repo}` 자동 일반화"를 확장 후보로 올려두고 있다. 본 SPEC은 그 항목의 첫 조각(폴더 피커)을 인도한다 — ADR-013은 이 연결을 명시한다
- **실패 후 재설계 경위** (§9 line 224, ADR-010이 모델): 해당 사항이 발생했다면 기록한다. run 단계에서 §B.4(`rfd::set_parent` 트레이트 불일치)로 `set_parent`를 제거했다면 그 경위가 여기 해당한다
- 관련 ADR 상호 참조: ADR-005(워크스페이스 상한), ADR-011(live cwd 상속 — 본 SPEC이 **재정의하지 않는다**는 결정), ADR-012(외부 열기 보안 — 같은 "자체 Rust 커맨드" 하우스 스타일 선례)

### S2 — `docs/GUIDE-command-palette.md` (**팔레트 커맨드 추가로 명시 발동**)

M4에서 `ws.root` / `ws.root.clear` 두 항목을 추가하므로 갱신이 선택이 아니다.

`DEVELOPMENT.md` line 118-122 「새 팔레트 커맨드」 레시피 3항이 요구하는 형태: **하는 일 / 쓰는 법 / 원리**를 비개발자용으로 상세히. 한 줄 항목만 넣는 것으로는 부족하다.

형식 — 파일의 기존 구조를 그대로 따른다:

```markdown
### Workspace: Set root folder…
- **하는 일**: …
- **쓰는 법**: …
- **원리**: …
```

**[HARD] 어휘 규약**: 이 파일은 §2 도입부에서 "**워크스페이스 = 책상**. 프로젝트마다 하나씩 두고, 각 책상에 칸을 여러 개 둡니다"라고 어휘를 선언하며, 기존 모든 항목이 이를 지킨다. 새 항목도 **책상 / 칸**을 쓴다 — "워크스페이스", "페인" 같은 기술 용어를 쓰면 비개발자용으로 쓰인 이 파일에서 유일하게 개발자 어휘를 쓰는 항목이 된다. 두 항목은 §2 「책상(워크스페이스) 다루기」 절에 들어간다. 검증은 AC-11 11-f / 11-f2.

### S3 — `docs/GUIDE-features-easy.md`

§6 한 줄 사전에 항목 추가(같은 레시피 3항). 비개발자용 한국어 설명. **S2와 동일한 어휘 규약(책상 / 칸)이 적용된다** — 이 파일도 비개발자 독자를 대상으로 한다.

### S4 — `README.md`

사용자 관점 요약 (§9 line 226).

### S5 — `docs/PLAN-M1-M2-roadmap.md`

해당 절에 "구현 완료 + 날짜 + 요약" 추가 (§9 line 225).

### S6 — `docs/DEVELOPMENT.md`

- line 26-27 상태 요약: "config schemaVersion **7**" → **8**, "Rust 테스트 92개" → 실제 개수, "autotest 32개 검사" → 실제 개수, "ADR-001~012" → **ADR-001~013**
- §9 line 228이 요구하는 모듈 지도 / 레시피 / 함정 갱신:
  - 모듈 지도에 `normalize_root_dir`, `set_root_dir`, `set_workspace_root`, `pick_folder` 반영
  - 함정(§ 트러블슈팅 표)에 §B.2(존재하지 않는 cwd의 조용한 폴백) 항목 추가 검토
- 참고: line 130-135 「config에 필드 추가 (스키마 마이그레이션)」 레시피가 이미 본 SPEC의 M1 절차(`#[serde(default)]` 필드 + `CONFIG_SCHEMA_VERSION` +1 + 이전 버전 arm 확장 + 픽스처 테스트)를 그대로 규정하고 있다. M1은 이 레시피의 이행이다

### S7 — `docs/ARCHITECTURE.md`

line 175 영속화 대상 목록에 `rootDir` 추가.

---

## §G 안티패턴 (하지 말 것)

| 안티패턴 | 이유 |
|---|---|
| `CONFIG_SCHEMA_VERSION`만 올리고 `config.rs:28`의 레거시 arm을 놔두기 | 사용자 워크스페이스 전량 소실 (§B.1) |
| 새 트리 순회 함수 작성 | `layout::collect_panes_mut`(layout.rs:193)가 이미 있다 |
| `get_mut` 이후에 경로 검증 | 잘못된 경로가 반쯤 적용된다. validate-before-mutate (R6) |
| `fs::canonicalize` 호출 | Windows `\\?\` 접두사가 UI와 `CommandBuilder::cwd`로 새어 나간다 |
| `#[tauri::command(async)]` 누락 | 네이티브 대화상자가 WebView2 이벤트 루프를 교착시킬 수 있다 (§B.3) |
| `set_workspace_root`에서 `ensure_sessions` 호출 | 띄울 세션이 없고, 기존 세션은 의도적으로 옛 cwd를 유지한다 |
| `autotest.ts`에서 `pick_folder` 호출 | 네이티브 모달이 headless 실행을 멈춘다 |
| 메뉴를 서브메뉴로 구현 | 바깥 mousedown 닫기 핸들러와 `sidebarBusy` 가드를 재설계해야 한다 |
| `.sb-colormenu-*` / `.sb-swatch` 클래스 이름 변경 | 불필요한 디프. id와 함수명만 바꾼다 |
| `split_pane`의 live cwd 상속을 시작 폴더로 재정의 | ADR-011 위반. spec.md §E에 명시된 비목표 |
| `apply_template`에서 루트 폴더 유도 | 템플릿이 방금 만든 팬 cwd를 조용히 덮어쓴다 |
| store 락을 잡은 채 `persist` 호출 | 락 순서 규율 위반 |
| `metas`만 반환하고 `workspace`를 빼기 | `main.ts:717`, `main.ts:1020`의 `leaf.cwd` 읽기가 stale해진다 |
| ADR-013 없이 SPEC을 닫기 | `DEVELOPMENT.md` §9의 "기능 하나 = ADR 하나" 의무 위반. `rfd` 결정이 plan.md에만 남아 SPEC 아카이브와 함께 묻힌다 |
| 팔레트 커맨드를 추가하고 `GUIDE-command-palette.md`를 건너뛰기 | §9 line 227 + line 118-122 레시피 3항의 명시 의무. 한 줄 항목만 넣는 것도 부족 — 하는 일/쓰는 법/원리가 필요하다 |
| ADR-013을 영어로 작성 | §9 line 220-222 — 모든 문서는 한국어 |
| 문서 작업을 M5(run 단계)에 섞기 | 실행 주체가 다르다(manager-develop vs manager-docs). §F.S로 분리되어 있다 |
| `listWorkspaces` arm을 손대지 않고 `WorkspaceMeta`에만 필드 추가 | 외부 브로커에 사용자 절대 경로가 무게이트 노출된다 (§A.4.1, `DEVELOPMENT.md:207` 불변식 3 위반) |
| 정규화와 `is_dir()` 검증을 한 함수에 다시 합치기 | UNC 가드와 후행 구분자 규칙을 검증할 방법이 사라진다 (§A.1) |
| `new_workspace`의 struct 리터럴을 건너뛰기 | `#[serde(default)]`는 역직렬화 전용. 컴파일 오류가 난다 (§A.5, §C) |

---

## §H 교차 참조

- `.moai/specs/SPEC-WORKSPACE-ROOT-001/spec.md` — 요구사항 R1~R11, 엣지 케이스, 비목표
- `.moai/specs/SPEC-WORKSPACE-ROOT-001/acceptance.md` — AC-1~AC-12, R↔AC 추적 행렬
- `src-tauri/src/config.rs:24-37` — 마이그레이션 match (§B.1의 대상)
- `src-tauri/src/lib.rs:43-54` — config 로드 실패 처리 (`fs::copy` → `.invalid` 사본, 기본 상태 기동). §B.1 인과 사슬의 마지막 고리
- `src-tauri/src/commands.rs` `handle_pipe_method` `listWorkspaces` arm — `metas()`의 두 번째 소비자 (§A.4.1의 대상)
- `src-tauri/src/commands.rs:952-969` — `workspace_as_template` ("cwds are kept literally"). spec.md §E 비목표의 근거
- `src-tauri/src/model.rs:42-44` — `allow_observe` 기본 차단 근거 주석 (§A.4.1)
- `src-tauri/src/state.rs:244-257` / `config.rs:113-128` — 나머지 두 `Workspace { }` 리터럴 (§C)
- `src-tauri/src/layout.rs:193-206` — `collect_panes_mut` (재사용 대상)
- `src-tauri/src/session.rs:462-466` — 조용한 cwd 폴백 (§B.2의 대상)
- `src-tauri/src/model.rs:14` — `CONFIG_SCHEMA_VERSION`
- `src-tauri/src/state.rs:166-173` — `set_color` (`set_root_dir`의 대칭 모델)
- `docs/DEVELOPMENT.md` line 130-135 「config에 필드 추가 (스키마 마이그레이션)」 — M1이 이행하는 레시피
- `docs/DEVELOPMENT.md` line 118-122 「새 팔레트 커맨드」 — M4 + S2가 이행하는 레시피
- `docs/DEVELOPMENT.md` line 124-128 「새 tauri 커맨드」 — M2/M3이 이행하는 레시피 (`lib.rs` 등록 누락은 런타임에서만 실패)
- `docs/DEVELOPMENT.md` §8 불변식 4 (line 211-212) — schemaVersion 불변식, R9/AC-1의 근거
- `docs/DEVELOPMENT.md` §9 문서 규칙 (line 218-228) — §F.S 전체의 근거
- `docs/DEVELOPMENT.md` §10 line 236 — "템플릿 UX: 폴더 피커 다이얼로그 …" 로드맵 항목. 본 SPEC이 그 첫 조각을 인도한다
- ADR-005 — 워크스페이스 상한 / PTY soft cap
- ADR-011 — live cwd 추적 및 `split_pane` 상속
