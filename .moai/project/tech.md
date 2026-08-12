# terminal-f — 기술 스택 & 검증 절차

> 이 문서는 **정확한 버전, 빌드/테스트/릴리스 절차, 검증 현황**을 다룬다.
> 제품 배경은 `product.md`, 코드 구조는 `structure.md` 참고.

## 1. 기술 스택 (정확한 버전)

### 프론트엔드 (`package.json`)

| 패키지 | 버전 |
|---|---|
| `@tauri-apps/api` | `^2.0.0` |
| `@xterm/xterm` | `^5.5.0` |
| `@xterm/addon-fit` | `^0.10.0` |
| `@xterm/addon-serialize` | `^0.13.0` |
| `@xterm/addon-web-links` | `^0.11.0` |
| `@xterm/addon-webgl` | `^0.18.0` |
| `@tauri-apps/cli` (dev) | `^2.0.0` |
| `typescript` (dev) | `^5.6.0` |
| `vite` (dev) | `^6.0.0` |

npm scripts: `dev` = `vite`(포트 5173, `strictPort`), `build` = `tsc --noEmit && vite build`,
`preview` = `vite preview`, `tauri` = `tauri`.

TypeScript 엄격도(`tsconfig.json`): `strict`, `noUnusedLocals`,
`noUnusedParameters`, `noFallthroughCasesInSwitch`, `isolatedModules`,
`noEmit`; target `ES2022`, `moduleResolution: Bundler`, lib
`["ES2022", "DOM", "DOM.Iterable"]`.

Vite 설정(`vite.config.ts`): `clearScreen: false`, 서버 포트 5173
(`strictPort`), build target `es2022`, outDir `dist`.

### 백엔드 (`src-tauri/Cargo.toml`)

edition 2021, lib 이름 `terminal_f_lib`(crate-type
`staticlib`/`cdylib`/`rlib`). 두 개의 바이너리: `terminal-f`(기본 실행,
`src/main.rs`), `bench`(`src/bin/bench.rs`).

| 크레이트 | 버전 |
|---|---|
| `tauri` | `2` |
| `serde` | `1` (features: `derive`) |
| `serde_json` | `1` |
| `portable-pty` | `0.9` |
| `uuid` | `1` (features: `v4`) |
| `which` | `7` |
| `sysinfo` | `0.33` |
| `interprocess` | `2` |
| `base64` | `0.22` |
| `arboard` | `3` |
| `png` | `0.17` |
| `tauri-plugin-opener` | `2` |

`[build-dependencies]`: `tauri-build = { version = "2" }`.
`[profile.release]`: `codegen-units = 1`, `lto = true`.

### Tauri 설정 (`src-tauri/tauri.conf.json`)

`productName: terminal-f`, `version: 0.1.1`, `identifier:
com.terminalf.app`. 윈도우: 1280×800, 최소 640×400. `beforeDevCommand: npm
run dev`, `devUrl: http://localhost:5173`, `beforeBuildCommand: npm run
build`, `frontendDist: ../dist`. `security.csp: null`. 번들 타겟:
`["nsis"]`.

캐패빌리티(`src-tauri/capabilities/default.json`): 윈도우 `main`에
`["core:default"]`만 부여 — 그 이상의 플러그인 캐패빌리티(예:
`opener:allow-open-url`)는 의도적으로 추가하지 않는다(ADR-012, 공격
표면 최소화).

## 2. 툴체인과 미고정 사실

**이 프로젝트는 버전 고정 파일을 두지 않는다** — `rust-toolchain.toml`,
`.nvmrc`, `package.json`의 `engines` 필드 모두 없음. 요구 사항은
README에만 서술되어 있다:

- Windows 10 1809 이상(ConPTY), WebView2 런타임
- Rust(stable), Node.js 18 이상

`docs/BENCHMARK.md`가 기록한 실측 실행 환경(2026-07-02, 참고용이며
고정 요구사항이 아님): Windows 11 Home 10.0.26200, Rust 1.96.0(디버그
프로파일), Node 22.14.0, Vite 6.4.3, 셸은 pwsh(PowerShell 7).

**품질 도구가 설정되어 있지 않다**: ESLint 없음, Prettier 없음,
`rustfmt.toml` 없음, `clippy.toml` 없음, `.editorconfig` 없음 — 전부
기본값에 의존한다. 커버리지 측정 도구도 구성되어 있지 않다
(`cargo-llvm-cov`, `cargo-tarpaulin` 등 미사용).

## 3. 빌드 / 테스트 / 릴리스 절차

### 개발 모드 실행

```powershell
npm install
npm run tauri dev        # vite + cargo 동시 구동
```

### 설치 파일 빌드

```powershell
npm run build             # tsc --noEmit + vite build
cd src-tauri; cargo build            # 백엔드 디버그 빌드
npx tauri build           # 배포용 NSIS 설치 파일 생성
```
`tauri.conf.json`의 `beforeBuildCommand: npm run build`가 `tauri build`
실행 시 자동으로 재실행된다.

### 필수 검증 순서 (반드시 이 순서로) — `docs/DEVELOPMENT.md` §3

```powershell
# 1) 백엔드 단위+통합 테스트
cd src-tauri; cargo test

# 2) 프론트 타입체크 + 번들
cd ..; npm run build

# 3) E2E autotest (실제 앱을 띄워 UI를 스크립트로 조작)
Remove-Item "$env:APPDATA\com.terminalf.app\config.json" -ErrorAction SilentlyContinue
Get-Process node,terminal-f -EA SilentlyContinue | Stop-Process -Force
$env:TERMF_AUTOTEST='1'; $env:TERMF_REPORT_PATH="$PWD\autotest-report.json"
npx tauri dev     # 자동 실행 후 스스로 종료, 리포트 JSON에서 "ok": true 확인
```

**"완료" 선언 규칙(프로젝트 자체 규칙, 반드시 준수)**: `cargo test` +
`npm run build` + autotest `ok: true`를 **전부 확인하기 전에는 완료라고
말하지 않는다**. 검증하지 못한 항목은 그대로 "미검증"이라고 보고한다.

### 성능 측정 (필요 시)

```powershell
cd src-tauri
cargo run --bin bench -- --soak-secs 600   # bench-report.json 생성
```

## 4. 테스트 현황

### Rust 테스트 — 총 **98개**

소스 내 단위 테스트(`#[test]`) 92개 + 통합 테스트 6개 = 98개.
파일별 분포(실측, 2026-08-05 기준):

| 파일 | 테스트 수 |
|---|---:|
| `automation.rs` | 14 |
| `session.rs` | 13 |
| `layout.rs` | 11 |
| `paste.rs` | 9 |
| `state.rs` | 9 |
| `config.rs` | 7 |
| `shellint.rs` | 7 |
| `template.rs` | 6 |
| `pipe.rs` | 5 |
| `spool.rs` | 4 |
| `audit.rs` | 3 |
| `commands.rs` | 3 |
| `model.rs` | 1 |
| **소스 내 소계** | **92** |
| `tests/pty_smoke.rs`(통합) | 5 |
| `tests/pipe_smoke.rs`(통합) | 1 |
| **총계** | **98** |

**문서 지연 참고**: `docs/DEVELOPMENT.md` §1(2026-07-05일자 서술)은 아직
"92개"로 기술하고 있다 — 실측 트리는 98개다. 이 문서(tech.md)가 실측
현재 값을 반영한다.

### 인앱 autotest — **32개 검사**

`autotest-report.json`(2026-07-15 기준, `ok: true`)이 32개
`checks` 항목을 전부 `true`로 기록: 승인 주입/감사 로그/복사
왕복/cwd 프롬프트 방출/echo/IME 출력 버퍼링/주입 및 관찰
거부-기본값/keep-alive/라이브 cwd split/멀티라인 청크/제안 생성 및
미승인 상태/OSC 52 복사/붙여넣기(클립보드 읽기/드롭/이미지)/규칙
저장·감사·타이머/split 생성/워크스페이스 전환 p95<150ms/템플릿(워크스페이스
생성·시작 커맨드·2페인)/URL 열기 게이트 및 위험 스킴 거부/워크스페이스
CRUD 등. 워크스페이스 전환 지연: p50 48.3ms / p95 55.5ms(30 샘플).

### 벤치마크 — `docs/BENCHMARK.md`

- **인앱 E2E harness**(2026-07-02): 워크스페이스 전환 p95 78.3ms
  (목표 < 150ms 충족), soak 46초 백엔드 RSS 증가율 ×1.031.
- **헤드리스 백엔드 harness**(K=2×N=2, 10분 soak): PTY 4개 spawn 78.7ms,
  출력 처리량 2.88 MiB/s, 백엔드 측 전환+리플레이 p50/p95 = 0.012ms/0.013ms,
  spawn 직후 RSS 11.5MB, **10분 soak RSS 증가율 ×1.124**(목표 < 1.5× 충족).
  overflow는 88,857 청크/4.1MB가 설계대로 카운트되며 드롭됨(메모리는
  평탄 유지).
- **Phase A 재검증**(사이드바+활동 폴링 추가 후): 워크스페이스 전환
  p50/p95/max = 93.8/111.5/113.3ms(여전히 < 150ms 충족).
- **M0에서 측정하지 않은 것**(주장 금지): M1 목표(라이브 PTY 16개,
  WebView2 포함 RSS < 1GB, 캐시된 전환 p95 < 50ms, 60분 soak),
  WebView2 포함 전체 앱 RSS, 대량 출력 렌더링의 부드러움(주관적 지표).

## 5. 테스트 함정 (실제로 겪은 것)

| 함정 | 원인/대응 |
|---|---|
| **헤드리스 pwsh 테스트가 무한 대기** | pwsh가 `ESC[6n`(DSR 커서 위치 질의)을 보내고 응답을 기다리며 블록됨 → `ESC[1;1R`을 응답하는 pump 헬퍼 필요(기존 smoke 테스트 참조) |
| **`cargo test`가 `STATUS_HEAP_CORRUPTION`(0xc0000374)로 죽음** | arboard가 쓰는 Windows OLE 클립보드는 스레드 친화적이라, 클립보드를 다루는 테스트 2개 이상이 병렬 스레드에서 동시 실행되면 힙 손상. `paste.rs::CLIPBOARD_LOCK`(Mutex)으로 직렬화. 새 클립보드 테스트 추가 시 반드시 이 락을 먼저 잡을 것 |
| **autotest는 비-hermetic(non-hermetic)** | config가 실행 간에 누적된다. 검사는 절대값 대신 상대값(+1 등)으로, 라벨은 `` `x-${Date.now()}` ``로 유일하게 작성. 깨끗한 시작이 필요하면 실행 전 `config.json` 삭제 |
| **`tauri dev`가 자동 종료된 후 vite(node)가 5173 포트를 물고 남음** | 다음 실행 전 `Get-Process node | Stop-Process -Force` |
| **합성 DOM 이벤트의 한계** | 합성 ClipboardEvent/KeyboardEvent는 리스너 로직만 검증한다. 실제 키 전달 경로(xterm의 keydown 가로채기 등)는 검증 못 한다 — ADR-010의 실패 사례가 이 함정을 실증한다 |

## 6. CI와 훅 — 사실 그대로

**GitHub Actions 워크플로는 `label-sync.yml` 하나뿐이다.** `.github/labels.yml`
로부터 저장소 라벨을 동기화하며, `workflow_dispatch`(dry-run 지원) 또는
`main` 브랜치의 `.github/labels.yml`/`label-sync.yml` 변경 push로
트리거된다. **빌드/테스트를 수행하는 CI 워크플로는 존재하지 않는다** —
검증은 전적으로 §3의 로컬 절차에 의존한다.

`.git_hooks/pre-push`(POSIX sh):
- `SKIP_MOAI_PREPUSH=1`으로 우회 가능(우회 시 `.moai/logs/prepush-bypass.log`에 기록됨).
- 저장소 루트에 `Makefile`이 있으면 `make -C <root> -s ci-local`을 실행하지만,
  **이 프로젝트에는 Makefile이 없으므로** `skip (no Makefile)`로 기록되고
  통과 처리된다.
- `moai`가 PATH에 있으면 push되는 커밋 subject를 `moai hook pre-push`로
  전달해 커밋 메시지 컨벤션 검사를 시도한다(이 검사 자체는 자체
  `enforce_on_push` 설정에 의해 게이트되므로 기본은 사실상 no-op).

## 7. config 스키마 버전업 절차 (현재 v7)

`config.rs::migrate()`는 `CONFIG_SCHEMA_VERSION`(현재 **7**)을 기준으로
동작한다. 스키마 이력의 정본은 `src-tauri/src/model.rs:8-13`의 주석이며,
아래 표는 그 주석을 그대로 옮긴 것이다(ADR 매핑은 각 마일스톤 표기에서 유도).

| 버전 | 추가된 필드 | 마일스톤 | 관련 ADR |
|---|---|---|---|
| v1 | 초기 스키마 | M0 | — |
| v2 | `Workspace.color` | Phase A | — |
| v3 | `PaneLeaf.labels`, `PaneLeaf.allowInjection` | M2.0 | ADR-006 |
| v4 | `Config.automation` 규칙 목록 | M2.1 | ADR-007 |
| v5 | `Rule.source` (tagged trigger — `gitDiff`/`timer`) | M2.1.5 | ADR-007 |
| v6 | `PaneLeaf.allowObserve` | M2.2 | ADR-008 |
| v7 | `PaneLeaf.startupCommand`, `Config.trustedRepos` | Phase B | ADR-009 |

`config.rs::migrate()`의 레거시 arm은 현재 `1 | 2 | 3 | 4 | 5 | 6`이며,
버전을 올릴 때 이 arm을 함께 넓히지 않으면 기존 설정 파일이 `other =>`
분기로 떨어져 `config.json.invalid`로 밀려나고 워크스페이스가 초기화된다
(`lib.rs` 부팅 경로). 아래 절차 2번이 이 지점을 가리킨다.

새 필드를 추가할 때 지켜야 하는 절차(`docs/DEVELOPMENT.md` §5):

1. `model.rs`에 `#[serde(default)]` 필드를 추가하고
   `CONFIG_SCHEMA_VERSION`을 +1 한다.
2. `config.rs::migrate()`의 버전 arm에 이전 버전 처리를 추가한다
   (additive 필드라면 버전 재스탬프만으로 충분).
3. **이전 버전 fixture 테스트를 추가한다**(기존 v1~v6 테스트 패턴을
   복사).
4. `PaneLeaf`에 필드를 추가했다면 `layout.rs`/`model.rs`의 모든 생성자를
   갱신한다(컴파일러가 누락을 잡아준다).

이 절차를 건너뛰면 구버전 `config.json`을 가진 사용자의 앱이 조용히
깨질 수 있다 — 이것이 `structure.md` §4의 불변식 4번이다.
