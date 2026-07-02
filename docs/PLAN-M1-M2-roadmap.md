# terminal-f 기능 확장 기획 (M1/M2 로드맵)

작성일: 2026-07-02. 코드 변경 없음 — 기획 문서.
전제: M0 아키텍처(backend-owned SessionRegistry, ring buffer + seq replay,
writePane 명시적 paneId, config.json)를 그대로 기반으로 한다.

---

## 1. 디자인 및 UI/UX 개선

### 1.1 테마 시스템

현재: Catppuccin Mocha 유사 색이 `styles.css`와 `terms.ts`에 하드코딩.

기획:
- **테마 토큰 스키마**: 하나의 JSON 토큰 세트가 (a) CSS 변수(`--bg`,
  `--panel`, `--accent`, `--divider`...)와 (b) xterm `ITheme`(bg/fg/cursor/
  selection + ANSI 16색)을 동시에 생성. 단일 소스에서 UI와 터미널 색 일치.
- 저장: `config.json`의 `ui.theme` (이미 `ui` 필드 존재, 스키마 변경 불필요
  → schemaVersion 유지 가능. 프리셋 이름 또는 인라인 토큰 오브젝트).
- 프리셋 4종 내장: Catppuccin Mocha/Latte, One Dark, Solarized Light.
  라이트 테마 1개 이상 필수(접근성).
- 런타임 전환: xterm은 `term.options.theme` 라이브 업데이트 지원 → 재마운트
  불필요. CSS 변수도 즉시 반영. 스냅샷(serialize)은 색 인덱스 기반이라 테마
  전환과 충돌 없음.
- 폰트/커서/패딩 설정: `ui.font{family,size,ligatures}`, `ui.cursorStyle`.

### 1.2 워크스페이스 탭 → cmux 스타일 개선

cmux에서 가져올 것:
- **활동 인디케이터**: inactive workspace의 pane에 새 출력이 쌓이면 탭에
  dot 뱃지. 구현 근거가 이미 있음 — backend ring의 `last_seq`와 frontend가
  마지막으로 본 seq의 차이를 주기적으로(예: 1s) 노출하는 경량 command
  `workspace_activity()` 추가. 이벤트 폭주 없이 폴링 1회/초로 충분.
- **상태 뱃지**: 세션 exited(빨간 dot), PTY spawn 실패(경고 아이콘).
  `SessionInfo.state`가 이미 있으므로 프론트 렌더링만 추가.
- **키보드 내비게이션**: `Ctrl+1..8` 워크스페이스 직접 전환, `Ctrl+Tab`
  최근 워크스페이스 토글(MRU 스택은 프론트 상태로 충분).
- **탭 재정렬**: 드래그로 순서 변경 → `reorder_workspaces(ids)` command
  (store의 Vec 순서가 곧 표시 순서, config에 자연 저장).
- **워크스페이스 컬러 라벨**: `Workspace.color?: string` (schemaVersion 2
  필요 — migration 스텁이 처음으로 실사용됨. optional 필드라 v1→v2는
  default 채우기만 하는 trivial migration).
- 레이아웃: **좌측 사이드바 기본 (확정, 2026-07-02)** — 단 **접기/펴기
  필수**. 접힘 상태에서는 아이콘/이니셜+활동 dot만 남는 슬림 레일(~44px),
  펼침 상태에서 이름·뱃지·컬러 라벨 표시(~220px, 드래그로 폭 조절).
  토글: 상단 햄버거 버튼 + `Ctrl+B`(사이드바 토글 관례). 접힘 상태와 폭은
  `ui.sidebar{collapsed,width}`에 저장. 사이드바 하단에 `+`(새 워크스페이스)
  와 설정 버튼 고정.

### 1.3 메뉴 / 커맨드 팔레트

- **커맨드 팔레트(Ctrl+Shift+P)를 1차 진입점으로** (VS Code 패턴). 모든
  동작(split/close/rename/테마 전환/§3 매크로 실행)을 명령 레지스트리에
  등록 → 팔레트는 그 위의 얇은 UI. §3 매크로가 자동으로 여기에 노출됨.
- 네이티브 메뉴(Tauri Menu API)는 최소한만: File(New workspace, Settings),
  View(theme, zoom), Help. 웹뷰 안 커스텀 메뉴바보다 OS 통합이 자연스러움.
- Pane 헤더(옵션): 프로세스명/cwd 표시는 OSC 추적(§2.4의 shell integration)
  전까지는 부정확하므로 M1에서는 pane 번호 + label(§3)만 표시.
- **Pane zoom** (Ctrl+Shift+Z): focused pane 임시 최대화. 레이아웃 트리는
  불변, 프론트 렌더링만 오버라이드 → backend 변경 불필요.

### 1.4 우선순위 (Phase A)

활동 인디케이터 > 테마 토큰+프리셋 > Ctrl+1..8 > 커맨드 팔레트 스켈레톤 >
zoom > 탭 재정렬 > 사이드바 모드.

**구현 상태 (2026-07-02): Phase A 전 항목 구현 완료.**
- 사이드바(접기/펴기, 폭 드래그, 드래그 재정렬, 컬러 라벨 우클릭 메뉴),
  활동/exited 뱃지(1s 폴링 `workspace_activity`), 테마 4종 프리셋(토큰 →
  CSS 변수 + xterm ITheme), 커맨드 팔레트, pane zoom, Ctrl+1..8, 폰트 크기.
- config schemaVersion 1→2 (`Workspace.color` 추가) + 실제 v1 fixture
  마이그레이션 테스트.
- **단축키 변경**: 사이드바 토글은 계획했던 `Ctrl+B` 대신 **`Ctrl+Shift+B`**.
  이유: plain Ctrl+B는 셸(readline backward-char)과 tmux prefix가 사용하는
  키라 PTY로 그대로 전달되어야 함 (스펙 5.3 "충돌 시 수정하고 기록" 준수).

---

## 2. 프롬프트 주입 / 브로커 에이전트 ("터미널 위에서 도는 무언가")

목표 시나리오: 한 repo에서 pane A = Claude Code, pane B = Codex 세션.
브로커가 git diff를 관찰하다가 변경 감지 시 Codex pane에 "이 변경 리뷰해"
프롬프트를 주입.

M0에 이미 있는 초석: `write_pane`(명시적 paneId), ring buffer가 원천 출력
저장소(ADR-003: watcher는 반드시 ring을 소비, serialize 금지), seq 기반
구독 가능 구조, §12 보안 원칙(allowlist 기반 설계 예약).

### 2.1 핵심 개념 모델

```
Watcher(감시)  ──event──▶  Broker Rule(규칙)  ──action──▶  Injector(주입)
  fs/git                     조건·디바운스·필터              대상 pane 결정
  pane output                템플릿 렌더링                   idle 게이트
  timer / exit                confirm 정책                   audit 기록
```

- **Watcher 소스**: ① filesystem/git(notify crate, `.git` 제외 감시 후
  `git status --porcelain`/`git diff --stat`으로 요약), ② pane output
  스트림(ring buffer를 seq부터 follow — 기존 replay 메커니즘의 구독 확장),
  ③ pane exit 이벤트, ④ 타이머.
- **Broker rule**: 선언적 설정(`automation.json` 또는 config의 `automation`
  섹션). 예:

```jsonc
{
  "watchers": [{
    "id": "git-to-codex-review",
    "source": { "type": "git-diff", "repo": "${workspaceFolder}", "debounceMs": 5000 },
    "action": {
      "type": "inject-prompt",
      "targetLabel": "codex",              // paneId가 아니라 label로 타겟팅
      "template": "파일 변경 감지:\n{{diffStat}}\n변경분을 리뷰해줘.",
      "requireIdle": true,                  // 대상 pane이 idle일 때만
      "mode": "confirm"                     // "confirm" | "auto"
    }
  }]
}
```

### 2.2 대상 지정: pane label (paneId 직접 참조 금지)

- paneId는 재시작마다 바뀌고 사용자가 알 수 없음 → **`PaneLeaf.labels:
  string[]`** 도입 (schemaVersion 2). 규칙은 label을 참조.
- label은 §3 매크로 템플릿에서 선언되거나 UI에서 pane에 수동 부착.
- 같은 label pane이 여럿이면: 기본 거부(모호성 에러), 규칙에
  `selection: "all" | "first"` 명시 시에만 다중/임의 대상 허용.

### 2.3 주입 안전성 (M0 §12의 구체화 — 이 설계의 핵심)

1. **Pane 단위 opt-in**: `allowInjection: true`인 pane에만 자동 주입 가능.
   기본 false. UI에 주입 허용 pane 시각 표시(테두리 아이콘).
2. **Idle 게이트**: TUI 에이전트(Claude Code/Codex)가 생성 중일 때 주입하면
   입력이 깨짐. 대상 pane의 ring `last_seq`가 N ms(기본 1500ms) 동안 정지
   상태일 때만 주입(출력 정지 = idle 휴리스틱). 향후 도구별 어댑터로 교체
   가능한 trait(`ReadinessDetector`)로 설계.
3. **Confirm 모드 기본**: 규칙 발화 시 토스트("git 변경 → codex pane에 리뷰
   요청 주입. [승인] [무시]") 후 주입. `auto`는 사용자가 규칙별로 명시해야.
4. **Audit log**: 모든 주입을 append-only 파일에 기록(시각, 규칙 id, 대상
   pane/session, 내용 해시+앞 200자). UI에서 최근 주입 이력 열람.
5. **루프 방지**: 주입으로 유발된 출력에 반응하는 자기 루프 차단 — 규칙별
   쿨다운(기본: 발화 후 30s 무시) + 동일 내용 연속 주입 dedup.
6. **Rate limit**: 규칙당 분당 최대 발화 수(기본 4).
7. **Kill switch**: 전역 자동화 일시정지 토글(상태바 아이콘 + 단축키).
   destructive 패턴(rm, git push --force 등) 포함 템플릿은 auto 모드 금지.
8. 주입 페이로드는 **bracketed paste**(`ESC[200~ ... ESC[201~`)로 감싸
   멀티라인 프롬프트가 즉시 실행되지 않게 하고, 제출(`\r`) 여부는 규칙의
   `submit: true|false`로 명시.

### 2.4 브로커 실행 위치 — 3단계 (인프로세스 → 외부 API)

**계층 구분이 핵심**: "무엇이 일어나면 고정 프롬프트를 넣어라"는 판단 없는
자동화는 **인프로세스 rule engine**에서 끝내고, LLM 판단처럼 로직이 터미널-f
밖에 있어야 하는 것만 **외부 API**로 뺀다.

- **M2a (내장 rule engine, 완료)**: git-diff / timer source → 게이트 통과
  주입. 배포·인증 불필요. 단순 스케줄러·조건반응은 여기서 끝난다.
- **M2b (외부 컨트롤 API, M2.2)**: named pipe
  (`\\.\pipe\terminal-f-<session>`) + 토큰 핸드셰이크로 줄 단위 JSON-RPC 노출:
  `listPanes` / `listWorkspaces`(상태), `subscribeOutput(paneId|label, fromSeq)`
  (스트리밍, file spool 기반), `injectPrompt(target, text, opts)`
  (`do_inject`, source=`pipe:<client>`), `listRules`/`runRule`.
  임의 언어로 짠 브로커가 소비한다. **이것이 장기 목표 5(plugin
  architecture)의 실체.** 모든 호출은 backend 게이트(kill switch / allowlist /
  idle / audit)를 통과 — 게이트가 backend에 있어 우회 불가.

기각한 대안:
- 브로커를 pane 안 프로세스로 돌리고 stdout을 명령으로 해석 — 파싱 취약,
  보안 경계 불명확.
- TCP localhost 소켓 — 포트 노출/네트워크 도달 위험. named pipe는 사용자
  SID ACL로 기본 격리.

### 2.4a `claude -p` AI 브로커 패턴 (M2.2의 대표 사용처)

판단이 필요한 자동화(무엇이 바뀌었는지 읽고, 리뷰 가치를 결정하고, 보낼
프롬프트를 생성)는 터미널-f 밖 브로커가 담당한다.

```
[terminal-f] pane 출력/‑git diff ──subscribeOutput──▶ [브로커(Node/Python)]
                                                          │ 컨텍스트 수집
                                                          ▼  claude -p "…" --output-format json  (헤드리스)
   [codex pane] ◀──injectPrompt(label="codex")───────────┘   do_inject 게이트+audit
```

**AI 실행 위치 규칙**:
- 판단 로직 = 브로커가 `claude -p`를 **아웃오브프로세스로 직접 호출**하고
  stdout(JSON)을 파싱. pane TUI 파싱 불필요, 깔끔한 분리.
- pane 안 세션 주입 = **사람이 보고 이어받게 할 때만**. TUI 출력 파싱으로
  상태를 읽는 건 취약하므로 판단 근거로 쓰지 않는다.
- 자연스러운 조합: 브로커가 헤드리스 `claude -p`로 판단 → 결과를 사용자가
  보는 pane에 주입 → 사람이 이어받음.

### 2.5 출력 구독의 데이터 보존 문제

ring은 1MiB oldest-drop이라 느린 외부 구독자는 유실 가능. M2b에서 구독자가
붙은 pane에 한해 **file spool**(ADR-004에 예약해둔 확장)을 활성화:
`%LOCALAPPDATA%/terminal-f/spool/<sessionId>.log`, 상한+로테이션. 구독자
없으면 기존 ring만 사용(비용 0 유지).

### 2.6 단계 분해

- **M2.0**: pane labels + allowInjection + idle 감지 + audit log +
  `inject_prompt` command(수동 트리거, 커맨드 팔레트에서 "Send prompt to
  labeled pane"). ← 자동화 없이도 유용, 위험 요소를 먼저 단단히.
  **구현 완료 (2026-07-02).** schemaVersion 3(labels/allowInjection),
  게이트 체인(kill switch → label/paneId 해석 → allowlist → idle 1500ms →
  bracketed paste 조건부 래핑), audit.log(JSONL) + 팔레트 열람, 상세는
  [ADR-006](ADR-006-injection-safety.md). bracketed paste는 세션 출력에서
  DECSET 2004를 추적해 대상 앱이 켠 경우에만 래핑.
- **M2.1**: 내장 git watcher + rule engine + confirm/auto + 쿨다운.
  **구현 완료 (2026-07-02).** 2초 폴링 방식(notify 미사용), git status/diff
  요약 → FNV 해시 기반 변경 감지, 순수 함수 `RuleRuntime::decide`(debounce
  2폴 안정화 → dedup → cooldown → 60초 rate limit, 전부 단위 테스트),
  confirm 기본(proposal 이벤트 → 승인 토스트 → M2.0 `do_inject` 재사용,
  audit source=rule-id) / auto 모드, `run_rule_now`(게이트 유지, 폴링 게이트만
  우회). 규칙은 config schemaVersion 4에 저장. 상세
  [ADR-007](ADR-007-automation-rule-engine.md).
- **M2.1.5** (rule engine 확장, timer source): "N분마다 pane에 주입" 같은
  판단 없는 스케줄링을 파이프 없이 인프로세스로 처리. `Rule.source`를
  tagged enum(`gitDiff` | `timer`)으로 확장 — serde default라 기존 v4 config
  무중단. timer는 git 요약 대신 경과시간으로 발화하며 기존 cooldown/rate
  limit/confirm/idle/audit를 그대로 재사용.
- **M2.2**: named pipe 컨트롤 API + 토큰 핸드셰이크 + file spool +
  `subscribeOutput` 프라이버시 게이트 + 예제 브로커(`claude -p` git-review)
  1개 동봉. **구현 완료 (2026-07-02).** interprocess 크레이트 기반 named
  pipe(`\\.\pipe\terminal-f-<id>.sock`), 줄 단위 JSON-RPC(request/response,
  서버 푸시 없음 — 브로커는 byte offset을 전진시키며 `readOutput` 폴링).
  기동 시 토큰+파이프명을 `control-api.json`(사용자 스코프)에 기록,
  첫 메시지 `auth` 필수. 메서드: `listPanes`/`listWorkspaces`/`readOutput`/
  `injectPrompt`(=do_inject, source=`pipe:<client>`)/`listRules`/`runRule`.
  출력 관찰은 pane별 `allowObserve` opt-in(schema v6, 기본 off) + per-session
  file spool(16MiB cap, byte-offset 커서, teardown 시 삭제). 모든 능력은
  UI와 동일한 backend 게이트 통과 — 파이프는 전송 계층일 뿐 우회 불가.
  참조 브로커: `examples/broker-git-review/`(Node, `claude -p` 헤드리스 리뷰).
  상세 [ADR-008](ADR-008-control-api-named-pipe.md).

### 2.7 M2.2 결정 사항 (2026-07-02 확정)

1. **timer source를 M2.1.5로 먼저 구현** — 단순 스케줄러를 파이프 없이 즉시
   충족. (채택: 아래 구현)
2. **`subscribeOutput` 프라이버시 게이트** — pane별 `allowObserve` opt-in을
   둔다(터미널 내용에 비밀이 있을 수 있으므로 `allowInjection`과 대칭).
   M2.2에서 pane 플래그 추가.
3. **참조 브로커 동봉** — `claude -p` git-review 브로커 예제(Node)를 넣어 API를
   실증. M2.2 산출물.
4. **인증** — 기동 시 토큰 파일(`%LOCALAPPDATA%/terminal-f/pipe-token`,
   사용자만 읽기) + 연결 첫 메시지 핸드셰이크. 클라이언트별 rate limit.

---

## 3. 매크로: 프리셋 멀티 스플릿 레이아웃 ("Project Profile")

목표: "매크로 실행 → 정해진 폴더 기준으로 미리 정의된 split 레이아웃 +
pane별 시작 명령이 한 번에 뜬다" (tmuxinator/tmuxp 포지션).

> **구현 완료 (Phase B, 2026-07-02).** 템플릿 = PaneNode 블루프린트 +
> `${param}`/`${env:VAR}` 변수 + pane별 `startupCommand`. `startupCommand`는
> 셸 교체(`command`)가 아니라 셸 준비 후(idle 800ms) 주입 → 명령 종료 후에도
> 셸 유지, allowInjection 게이트 무관(사용자 본인 액션이라 audit 대상 아님).
> 적용은 항상 새 워크스페이스 생성(caps·16 pane·invariant 검사). 저장 2계층:
> 전역 `<config>/templates/*.json`(신뢰) + repo 로컬
> `<repo>/.terminal-f/profile.json`(명령 포함 시 workspace trust 확인 →
> `trustedRepos`, config schema v7). 팔레트: Apply "<name>" / Save current
> layout / Apply repo profile. 상세 [ADR-009](ADR-009-project-templates.md).

### 3.1 템플릿 스키마

기존 PaneNode 트리를 그대로 재사용하되 런타임 상태를 빼고 변수를 더한 형태:

```jsonc
{
  "name": "ai-pair-dev",
  "params": [{ "name": "repo", "type": "folder", "prompt": "대상 repo 선택" }],
  "root": {
    "kind": "split", "direction": "row", "ratio": 0.5,
    "first":  { "kind": "pane", "cwd": "${repo}", "labels": ["claude"],
                "startupCommand": "claude", "allowInjection": false },
    "second": {
      "kind": "split", "direction": "column", "ratio": 0.6,
      "first":  { "kind": "pane", "cwd": "${repo}", "labels": ["codex"],
                  "startupCommand": "codex", "allowInjection": true },
      "second": { "kind": "pane", "cwd": "${repo}/frontend", "labels": ["shell"] }
    }
  },
  "automation": ["git-to-codex-review"]   // §2 규칙을 함께 활성화(선택)
}
```

핵심 설계 결정:
- **`startupCommand` ≠ `command`**: 기존 `command`는 셸 자체 교체(셸 대신
  그 프로그램이 PTY 주인 → 종료 시 pane 죽음). `startupCommand`는 기본 셸을
  띄운 뒤 셸이 준비되면 명령을 주입(§2.3의 idle 게이트 재사용!) → 명령 종료
  후에도 pane이 살아있는 셸로 남음. 템플릿에서는 startupCommand를 기본으로
  권장. **여기서 §2와 §3이 같은 기반(readiness 감지 + 주입 경로)을 공유**
  하므로 M2.0을 먼저 만들면 매크로 시작 명령은 공짜에 가깝다.
- 변수: `${repo}` 등 params 치환 + `${workspaceFolder}`, `${env:X}` 내장.
- 트리 형태가 기존 모델과 동일 → `apply_template`은 layout.rs의 기존 연산
  조합으로 구현 가능(새 invariant 없음).

### 3.2 저장 위치 — 2계층

1. **전역**: `%APPDATA%/com.terminalf.app/templates/*.json` — 개인 매크로.
2. **repo 로컬**: `<repo>/.terminal-f/profile.json` — 팀 공유용(커밋 가능).
   단, **repo 로컬 프로필은 VS Code workspace-trust 패턴 필수**: 처음 여는
   repo의 startupCommand는 신뢰 확인 다이얼로그 전까지 실행 금지(악성 repo가
   clone 직후 임의 명령 실행하는 벡터 차단).

### 3.3 실행 UX

- 커맨드 팔레트: "Apply template: ai-pair-dev" (§1.3과 합류).
- 탭 `+` 버튼 우클릭/롱프레스 → 템플릿 목록.
- **현재 레이아웃을 템플릿으로 저장**: config 저장 로직이 이미 트리를
  sanitize해서 직렬화하므로(세션 상태 제거) 거의 공짜. cwd를 `${repo}`로
  일반화할지 물어보는 단계만 추가.
- CLI/딥링크: `terminal-f.exe --template ai-pair-dev --param repo=C:\work\x`
  및 `terminal-f://` URL 스킴(나중에). 부팅 시 단일 인스턴스 포워딩 필요
  (tauri-plugin-single-instance).
- 적용 대상: 항상 **새 workspace 생성**에 적용(기존 workspace 덮어쓰기는
  세션 파괴라 혼란 — 명시적 "replace" 옵션은 confirm 필수).

### 3.4 제약/정합성

- live PTY soft cap(32)과 상호작용: 템플릿의 pane 수가 cap을 넘기면 부분
  적용 + 경고(기존 ensure_sessions 경고 경로 재사용).
- pane 수 상한: 템플릿 검증 시 workspace당 16 pane 초과 거부(M0 레이아웃
  용량 정책과 일치).
- 템플릿도 layout invariant 검사(`check_invariants` 재사용)를 로드 시 통과
  해야 함.

---

## 4. 통합 로드맵과 의존 관계

```
Phase A (M1, UI):        테마 토큰 / 활동 인디케이터 / Ctrl+1..8 / 팔레트 / zoom
Phase B (M1.5, 매크로):  schemaVersion 2 (labels, color) → 템플릿 스키마
                         → apply_template → 팔레트 노출 → repo-trust
Phase C (M2.0~2.1, 자동화): allowInjection + idle 게이트 + audit
                         → startupCommand가 이 기반을 소비 (B와 C.0 순서 조정 가능)
                         → 내장 git watcher + rule engine
Phase D (M2.2, 플러그인): named pipe 컨트롤 API + file spool + 예제 브로커
```

의존의 핵심: **idle 감지 + 안전한 주입 경로(M2.0)가 매크로 startupCommand와
브로커 주입 양쪽의 공통 기반**이므로, M2.0을 Phase B 직전 또는 병행으로
당기는 것을 권장. schemaVersion 2 마이그레이션(labels/color/allowInjection
추가)은 한 번에 묶어서 수행.

## 5. 확정된 결정 (2026-07-02 사용자 확인)

1. **탭 UI: 좌측 사이드바 기본, 접기/펴기 필수** (`Ctrl+B` 토글, 접힘 시
   슬림 레일). §1.2에 반영.
2. **주입 confirm: 규칙별 confirm이 기본**, auto는 규칙에 명시적으로
   설정한 경우만.
3. **repo 로컬 프로필(.terminal-f/) 지원 + workspace trust 다이얼로그 수용.**
4. **브로커: 2단계 권장안 채택** — 내장 rule engine(M2.1) 먼저, 외부
   프로세스용 named pipe 컨트롤 API(M2.2)는 그 다음.

비개발자 대상 설명은 [GUIDE-features-easy.md](GUIDE-features-easy.md) 참조.
```
