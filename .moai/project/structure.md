# terminal-f — 구조 개요

> 이 문서는 **코드가 어떻게 배치되고 서로 어떻게 통신하는지**를 다룬다.
> 제품 배경은 `product.md`, 기술 스택/빌드 절차는 `tech.md` 참고.

## 1. 디렉터리 트리

### 최상위

```
terminal-f/
├── src/                    # 프론트엔드 (TypeScript + xterm.js, vanilla)
├── src-tauri/               # 백엔드 (Rust, Tauri 2)
│   ├── src/
│   ├── capabilities/        # Tauri 캐패빌리티 선언 (default.json)
│   ├── tests/                # 통합 테스트 (pty_smoke.rs, pipe_smoke.rs)
│   └── icons/
├── docs/                    # ADR-001~012, ARCHITECTURE.md, DEVELOPMENT.md, GUIDE-*, BENCHMARK.md, PLAN-*
├── examples/                 # templates/, broker-git-review/ (참조용 예제, 앱이 실행하지 않음)
├── .github/workflows/        # label-sync.yml (빌드/테스트 CI 없음)
├── .git_hooks/                # pre-push (make ci-local 시도, Makefile 없어 스킵)
├── index.html                 # Vite 진입점
├── package.json / tsconfig.json / vite.config.ts   # 프론트 빌드 설정
├── autotest-report.json / bench-report.json          # 최근 검증/벤치마크 결과물
└── .moai/                     # MoAI-ADK 워크플로 상태 (SPEC, config 등)
```

### `src/` (프론트엔드)

| 파일 | 책임 |
|---|---|
| `main.ts` | 오케스트레이터: 부팅, 워크스페이스 전환, 팔레트 커맨드 전부 등록, 주입/자동화/템플릿 UI 플로우, drag-drop 배선 |
| `terms.ts` | xterm 인스턴스 수명주기, 유일한 출력 sink `writeOutput`, Ctrl+V 가로채기, OSC 52/9 핸들러, IME 조합 버퍼링 |
| `autotest.ts` | 스크립트 기반 인앱 E2E 시나리오(32개 검사) |
| `sidebar.ts` | 워크스페이스 사이드바 UI |
| `themes.ts` | 테마 토큰 — CSS 변수와 xterm `ITheme` 양쪽에 공급 |
| `types.ts` | 백엔드 serde 모델과 camelCase로 1:1 대응하는 공유 타입 |
| `modal.ts` | 인앱 모달(네이티브 `window.prompt` 회피) |
| `ipc.ts` | 프론트↔백엔드 유일한 통신 표면 — 모든 백엔드 커맨드의 typed wrapper |
| `palette.ts` | 명령 팔레트 |
| `renderer.ts` | 페인 트리 → DOM 렌더링, 기존 xterm 호스트 재부착(reparent) |
| `commands.ts` | 커맨드 레지스트리 |
| `util.ts` | 유틸리티 |
| `styles.css` | 스타일시트 |

의존 방향: `main.ts` → `ipc.ts`(유일한 백엔드 통로) + `terms`/`renderer`/`sidebar`/`palette`/`modal`/`themes`/`types`/`util`.

### `src-tauri/src/` (백엔드)

| 파일 | 책임 |
|---|---|
| `main.rs` | 바이너리 진입점 |
| `lib.rs` | 앱 부팅: config 로드 → `AppState` 구성 → emitter/automation/pipe 스레드 기동 → **모든 tauri 커맨드 등록**(`invoke_handler`) |
| `model.rs` | serde 모델(Config, Workspace, PaneNode 등), `CONFIG_SCHEMA_VERSION = 7` |
| `layout.rs` | 페인 이진 트리 연산(split/close/resize/collect) + `check_invariants`. `model`에만 의존하는 순수 리프 모듈 |
| `state.rs` | `AppState` + `WorkspaceStore`(CRUD, 상한, 신뢰) + 주입 대상 결정(`resolve_inject_target`) |
| `config.rs` | `config.json` 로드/저장/마이그레이션. 스키마 버전별 fixture 테스트 위치 |
| `session.rs` | `SessionRegistry` + `PtySession`, reader 스레드, `scan_cwd`(라이브 cwd 파싱) |
| `output.rs` | 16ms 배칭 출력 emitter |
| `commands.rs` | 모든 `#[tauri::command]` + `do_inject` 공유 헬퍼 + `handle_pipe_method`(컨트롤 API 라우팅) — **허브 모듈** |
| `automation.rs` | 규칙(gitDiff/timer) + 순수 판단 로직(`RuleRuntime::decide*`) |
| `audit.rs` | 주입 감사 로그(JSONL append/tail) |
| `pipe.rs` | named pipe 전송 계층. **`AppState`를 모른다** — 핸들러 클로저 주입 방식 |
| `spool.rs` | 관찰용 페인별 파일 스풀 |
| `template.rs` | 템플릿 스키마/변수치환/검증/`build_tree` |
| `paste.rs` | 클립보드 브리지(arboard) + PNG 인코딩 + paste 파일 관리 |
| `shellint.rs` | pwsh `$PROFILE` 셸 통합 스니펫 설치/제거/업그레이드 |
| `bin/bench.rs` | 헤드리스 처리량/지연 벤치마크 |

의존 방향: `main.rs` → `lib.rs`가 전부를 구성한다. `commands.rs`는 layout/model/session/state/automation/audit/template/paste/spool/shellint에 의존하는 **허브**이며, pipe 메서드를 캐패빌리티로 매핑하는 **유일한 지점**이라 파이프가 게이트를 우회할 수 없다. `layout.rs`는 `model`에만 의존하는 순수 함수 리프. `pipe.rs`는 핸들러 클로저를 주입받는 디커플링된 전송 계층. `automation.rs`는 순수 판단 로직과 부수효과를 분리한다.

## 2. 모듈 경계와 의존 방향

### 책임 분리표

| 관심사 | 담당 |
|---|---|
| PTY 프로세스 생명주기, 세션 레지스트리 | **백엔드** (`session.rs`) |
| 페인 트리 / 워크스페이스 상태 + 변경 | **백엔드** (`state.rs`, `layout.rs`) |
| 원시 출력 캡처(크기 제한 링 버퍼) | **백엔드** (`session.rs`) |
| 영속화(`config.json`) | **백엔드** (`config.rs`) |
| 레이아웃 렌더링, 구분선, 탭 | **프론트엔드** (`renderer.ts`, `main.ts`) |
| 포커스된 페인의 시각 상태 | **프론트엔드** (백엔드는 `activePaneId`만 미러링) |
| 터미널 화면 렌더링, 시각적 스크롤백 | **프론트엔드** xterm.js |
| 시각 스냅샷 / 복원 | **프론트엔드** serialize addon (ADR-003) |

### 백엔드 소유 상태 패턴 (ADR-001)

Rust 백엔드가 PTY 세션과 페인 트리/워크스페이스 상태 양쪽 모두의 **단일
진실 공급원**이다. 프론트엔드는 프로세스 핸들을 절대 보유하지 않으며,
레이아웃을 로컬에서 절대 변경하지 않는다. 모든 split/close/resize는
Tauri 커맨드로 수행되고, 프론트엔드는 반환된 권위 있는 트리를 기준으로
다시 렌더링한다.

`SessionRegistry` 구조:

```
sessions:               HashMap<SessionId, Arc<PtySession>>
pane_to_session:        HashMap<PaneId, SessionId>
workspace_to_sessions:  HashMap<WorkspaceId, Vec<SessionId>>
active_workspace:       Option<WorkspaceId>
```

이 설계를 택한 이유(검토했던 대안): 프론트엔드 소유 세션은 WebView
리로드/HMR 시 프로세스가 고아가 되는 문제로 기각; 백엔드 세션 + 프론트
소유 레이아웃 트리는 페인 닫기 시 원자적 2단계 처리가 필요해 상태 어긋남
위험이 있어 기각; 별도 데몬 프로세스는 M0 범위 밖으로 보류(레지스트리 API는
추후 트레이트 경계 뒤로 옮길 수 있게 설계).

## 3. 경계를 넘는 3가지 흐름

### 출력 흐름

```
ConPTY output
  → reader thread (8 KB 읽기, UTF-8 경계 복구)
  → 세션별 링 버퍼(1 MiB / 1024 청크, oldest-drop, seq 부여)
  → [활성 워크스페이스만] 16ms 배치 emitter → "pty-output" 이벤트
    { workspaceId, paneId, sessionId, seq, data }
  → terms.writeOutput (유일한 출력 sink) → xterm.write
```

비활성 워크스페이스는 이벤트를 받지 않는다 — 출력은 링 버퍼에 누적되고
전환 시 `replay_pane(paneId, fromSeq)`로 리플레이된다.

### 입력 흐름

```
keyboard → xterm onData → invoke write_pane(paneId, data) → PTY writer
```

`write_pane`은 언제나 명시적 `paneId`를 요구한다. `broadcast_write`는
M0에서 구현되지 않았다(로드맵 M1 항목).

### 워크스페이스 전환 흐름

```
사용자가 탭 클릭
  → 프론트: 각 페인의 xterm을 시각 스냅샷으로 직렬화(serialize addon, 스크롤백 1000줄 제한)
  → 프론트: xterm 인스턴스 완전 언마운트(dispose) — display:none과 절대 혼용하지 않음
  → 백엔드: switch_workspace(id) — 활성 워크스페이스 설정, PTY는 건드리지 않음, 누락 세션 lazy-spawn
  → 프론트: 페인 트리 렌더 → xterm 재생성 → 스냅샷 복원
  → 프론트: 페인별 replay_pane(paneId, lastSeq) 호출
  → 백엔드: 리플레이 종료 seq부터 라이브 방출 재무장(replay_synced)
  → 프론트: 포커스 복원
```

세션은 워크스페이스 전환 시 **절대 종료하지 않는다**(keep-alive,
ADR-002). 순서 보장: 세션별 단조 증가 `seq`. 프론트엔드는 마지막 seq를
추적하고, 리플레이는 그 지점부터 재개되며, 라이브 방출은 리플레이 종료
지점부터 재시작된다 — 스냅샷/리플레이/라이브 경계를 넘어 데이터가 절대
중복되거나 순서가 뒤바뀌지 않는다.

## 4. 불변식 (깨면 안 되는 것)

`docs/DEVELOPMENT.md` §8 기준, 코드 변경 시 반드시 지켜야 하는 6개
불변식:

1. **페인 트리**: 모든 Split은 자식 정확히 2개, ratio ∈ [0.1, 0.9], 페인
   id 유일. 트리 변경 후 `check_invariants` 통과 필수. 마지막 페인은
   닫기 거부.
2. **주입 기본 차단**: `allowInjection`/`allowObserve` 기본값은 `false`.
   새 기능이 이 기본값을 바꿔서는 안 된다.
3. **게이트 우회 금지**: 모든 자동 주입은 `do_inject` 경유. 사용자 본인
   타이핑(`write_pane`)과 템플릿 `startupCommand`만 예외(주입이 아니므로
   게이트/감사 대상이 아님).
4. **schemaVersion**: config 필드 추가마다 버전 +1과 fixture 테스트가
   필요하다. 구버전 config가 조용히 깨져서는 안 된다.
5. **백엔드 소유권**: 프론트엔드는 레이아웃/세션 상태를 로컬에서 절대
   변경하지 않는다.
6. **출력 순서**: 링 버퍼의 seq는 연속·단조 증가. 리플레이는 seq 기반.

## 5. 런타임 파일 위치

`%APPDATA%\com.terminalf.app\`:

| 파일/폴더 | 내용 |
|---|---|
| `config.json` | 전체 상태(워크스페이스, 페인 트리, ui, automation, trustedRepos). `schemaVersion` 필드로 마이그레이션 |
| `audit.log` | 주입 감사 로그(JSONL). `source` 필드로 manual/rule:<id>/pipe:<client> 구분 |
| `control-api.json` | named pipe 이름 + 인증 토큰. 앱 기동마다 재생성 |
| `templates/*.json` | 전역 템플릿(팔레트 "Save current layout as template"으로 생성) |
| `paste/img-*.png` | 붙여넣은 클립보드 이미지(최근 20개 유지) |
| `spool/*.log` | `allowObserve` 켜진 페인의 출력 스풀(세션 종료 시 삭제) |

repo 로컬: `<repo>/.terminal-f/profile.json` (템플릿, 신뢰 게이트 대상).

## 6. 문서화 의무 (`docs/DEVELOPMENT.md` §9)

새 기능/변경을 할 때 이 프로젝트가 스스로에게 부과하는 절차적 의무:

- **모든 문서는 한국어로 작성한다.** 코드 식별자·파일명·명령어는 영어
  유지(2026-07-02에 ADR 전체와 ARCHITECTURE.md를 한글화했음).
- **기능 하나 = ADR 하나**(`docs/ADR-XXX-*.md`): 배경 → 결정 → 트레이드오프
  → 테스트 구조. **실패했다가 재설계한 경우 그 경위도 기록**한다
  (ADR-010이 이 패턴의 모범 사례).
- 기능 완성 시 다음 문서도 함께 갱신한다:
  - `docs/PLAN-M1-M2-roadmap.md`의 해당 절 — "구현 완료 + 날짜 + 요약"
  - `README.md` — 사용자 관점 요약
  - `docs/GUIDE-features-easy.md` — 비개발자용 기능 설명
  - `docs/GUIDE-command-palette.md` — 팔레트 커맨드일 경우 원리까지 상세
    설명
  - `docs/DEVELOPMENT.md`의 모듈 지도 / 레시피 / 디버깅 함정 섹션

이 규칙은 SPEC 작업 시 "완료" 선언 이전에 문서 갱신이 누락되지 않았는지
확인하는 체크리스트로 쓸 수 있다.
