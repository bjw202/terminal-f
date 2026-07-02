당신은 이 프로젝트의 principal architect 겸 senior implementation agent입니다.
사용자의 요구를 단순히 그대로 구현하지 말고, 먼저 기술적 타당성을 검토한 뒤 아키텍처 계약을 확정하고, 그 계약에 맞춰 M0를 구현하세요.

중요:
아래 요구사항 중 일부는 초기 설계 가설입니다. 구현 전에 반드시 충돌, 잘못된 기술 가정, 과도한 성능 목표, 라이브러리 제약을 검토하세요.
요구사항이 기술적으로 부적절하면 그대로 따르지 말고, 더 안전한 대안을 선택한 뒤 ADR에 이유를 기록하세요.
단, “backend-owned PTY session registry”, “workspace keep-alive”, “pane tree invariant”, “output backpressure”, “build/test/benchmark 검증”은 반드시 만족해야 합니다.

---

# 0. 프로젝트 목표

Windows 네이티브 데스크톱 터미널 에뮬레이터의 핵심 코어를 만든다.

장기 목표:

1. cmux/tmux 스타일의 자유로운 split pane layout
2. 프로젝트별 workspace 관리
3. pane별 PTY stdin 주입
4. pane별 stdout/stderr 캡처
5. 향후 watcher / automation / plugin architecture 확장

이번 /goal의 실제 구현 범위는 M0입니다.
M1/M2 기능은 설계 여지를 남기되, M0에서 과도하게 구현하지 마세요.

---

# 1. 기술 스택

Backend:

* Rust
* Tauri 2.x
* portable-pty
* Windows ConPTY

Frontend:

* TypeScript
* xterm.js
* xterm.js WebGL addon, 단 fallback 가능해야 함

IPC:

* Tauri commands
* Backend-to-frontend events

우선 대상 OS:

* Windows

---

# 2. 최우선 아키텍처 원칙

다음 원칙은 반드시 지켜야 합니다.

1. Backend가 PTY session ownership의 source of truth이다.
2. Frontend는 layout rendering, focused pane state, xterm visual state만 담당한다.
3. Pane layout state와 PTY session state를 분리한다.
4. Workspace 전환 시 PTY process를 죽이면 안 된다.
5. Pane close 시 해당 pane의 PTY session은 graceful terminate한다.
6. Workspace delete 시 해당 workspace의 모든 PTY session을 graceful terminate한다.
7. stdout/stderr raw capture와 xterm visual snapshot은 분리한다.
8. xterm.js serialize는 출력 캡처 원천 저장소가 아니라 visual snapshot / restore 용도로만 사용한다.
9. 비활성 workspace의 output도 유실 정책이 명시되어야 한다.
10. 완료 선언 전에 build, test, smoke test, benchmark를 실행하고 결과를 보고해야 한다.
11. 테스트를 실행하지 못했거나 benchmark를 실행하지 못했으면 “완료”라고 말하지 말고, 미검증 항목으로 명시해야 한다.

---

# 3. 먼저 수행할 일: Requirement Audit

구현을 시작하기 전에 다음을 먼저 수행하세요.

## 3.1 현재 repo 조사

* 현재 repo 구조 파악
* 사용 중인 frontend framework 확인
* Tauri 버전 확인
* Rust crate 구성 확인
* 기존 terminal / pane / workspace 관련 코드가 있으면 재사용 가능성 검토

## 3.2 기술 가정 검토

다음 항목을 반드시 검토하고, 잘못된 가정이면 수정하세요.

* portable-pty에서 ConPTY 관련 flag를 어느 수준까지 제어할 수 있는가?
* xterm.js serialize를 output capture로 쓰면 안 되는 이유
* inactive workspace를 display:none으로 유지할지 unmount할지
* 16 workspace × 16 pane = 256 PTY를 live keep-alive 목표로 볼 수 있는지
* Tauri event를 chunk마다 emit할 때의 성능 위험
* WebView2 RSS가 전체 memory budget에 주는 영향

## 3.3 설계 결정 기록

다음 ADR을 작성하세요.

* ADR-001: Backend-owned PTY Session Registry
* ADR-002: Workspace keep-alive policy
* ADR-003: Output capture와 visual snapshot 분리
* ADR-004: Backpressure와 ring buffer 정책
* ADR-005: Live PTY soft cap과 memory policy

ADR에는 다음을 포함하세요.

* 선택한 방식
* 버린 대안
* 선택 이유
* trade-off
* 향후 변경 가능성

---

# 4. M0 구현 범위

M0에서 반드시 구현할 것:

1. Binary tree split pane layout
2. Workspace CRUD
3. Workspace switching
4. Pane당 하나의 PTY session
5. xterm.js 기반 terminal rendering
6. focused pane에 대한 stdin write
7. backend PTY output event
8. inactive workspace keep-alive
9. config.json save/load
10. 최소 테스트
11. 최소 benchmark script

M0에서 구현하지 않아도 되는 것:

1. 진짜 plugin system
2. watcher-triggered stdin injection
3. 복잡한 shell profile manager
4. 완전한 scrollback persistence
5. 256 live PTY stress test
6. production-grade migration system
7. 고급 drag resize UI
8. remote terminal
9. SSH integration

M0에서 구현하지 않는 항목은 interface 또는 TODO 수준으로만 남기고, 구현했다고 말하지 마세요.

---

# 5. Binary Tree Split Pane Engine

정형 grid가 아니라 recursive binary tree layout을 사용하세요.

## 5.1 Data model

개념 모델:

```ts
type PaneId = string;
type WorkspaceId = string;
type SessionId = string;

type PaneNode =
  | {
      kind: "pane";
      id: PaneId;
      sessionId: SessionId | null;
      cwd: string;
      command: string | null;
    }
  | {
      kind: "split";
      id: string;
      direction: "row" | "column";
      ratio: number;
      first: PaneNode;
      second: PaneNode;
    };
```

direction 의미:

* row: 좌우 분할, CSS flex-direction: row
* column: 상하 분할, CSS flex-direction: column

ratio 의미:

* first child의 flex-basis 비율
* second child는 1 - ratio
* ratio 범위는 0.1 ~ 0.9로 clamp

## 5.2 Operations

다음 operation을 구현하세요.

* splitPane(targetPaneId, direction)

  * target pane을 split node로 교체
  * 기존 pane은 first child
  * 신규 pane은 second child
  * 기본 ratio는 0.5

* closePane(paneId)

  * 대상 pane 제거
  * sibling을 parent 위치로 승격
  * unary split node가 남지 않게 invariant 유지
  * 마지막 pane은 닫지 않거나, 닫을 경우 workspace에 새 empty pane을 생성하는 정책을 명시

* resizeSplit(splitId, ratio)

  * ratio clamp
  * layout state 업데이트

## 5.3 Keyboard shortcuts

* Ctrl+Shift+D: focused pane 좌우 분할, direction=row
* Ctrl+Shift+-: focused pane 상하 분할, direction=column
* Ctrl+Shift+W: focused pane close

단축키 충돌이 있으면 repo 환경에 맞게 수정하고 이유를 기록하세요.

## 5.4 Rendering

* recursive component로 PaneNode를 렌더링
* split node는 flex container
* pane node는 xterm container
* active/focused pane을 시각적으로 구분
* pane tree invariant가 깨지지 않게 frontend/backend 중 어느 쪽에서 layout mutation을 관리할지 명확히 결정

---

# 6. Workspace Management

Workspace는 독립적인 PaneNode tree를 가집니다.

## 6.1 Workspace model

개념 모델:

```ts
type Workspace = {
  id: WorkspaceId;
  name: string;
  root: PaneNode;
  activePaneId: PaneId | null;
  createdAt: number;
  updatedAt: number;
};
```

## 6.2 Commands

다음 명령을 구현하세요.

* createWorkspace(name)
* renameWorkspace(workspaceId, name)
* deleteWorkspace(workspaceId)
* switchWorkspace(workspaceId)
* listWorkspaces()

## 6.3 Caps

* workspace soft cap: 8
* workspace hard cap: 16
* live PTY soft cap: 32
* 16 workspace × 16 pane = 256은 layout hard capacity일 뿐, live PTY keep-alive 보장 목표가 아닙니다.

soft cap 초과 시:

* 경고
* lazy spawn
* spawn 거부
* 또는 명시적 사용자 확인

중 하나를 선택하고 ADR에 기록하세요.

## 6.4 Switching policy

Workspace 전환 시:

1. 현재 workspace의 xterm visual snapshot을 저장한다.
2. 현재 workspace의 xterm DOM은 unmount하거나 이에 준하는 메모리 절감 정책을 적용한다.
3. backend PTY session은 종료하지 않는다.
4. target workspace의 xterm을 mount한다.
5. 저장된 visual snapshot이 있으면 복원한다.
6. backend에 쌓인 pending output을 replay한다.
7. focus state를 복원한다.

주의:

* display:none과 unmount 정책을 혼용하지 마세요.
* memory budget을 우선하면 inactive workspace xterm DOM unmount를 기본 정책으로 고려하세요.
* 선택한 정책과 trade-off를 ADR에 기록하세요.

---

# 7. PTY Session Manager

Backend에 SessionRegistry를 둡니다.

## 7.1 개념 구조

```rust
SessionRegistry
  - sessions: HashMap<SessionId, PtySession>
  - pane_to_session: HashMap<PaneId, SessionId>
  - workspace_to_sessions: HashMap<WorkspaceId, Vec<SessionId>>
```

PtySession 개념 필드:

* session_id
* workspace_id
* pane_id
* lifecycle_state: starting | running | exited | closing
* child process handle
* writer handle
* reader task handle
* output ring buffer
* pending channel
* seq counter
* cwd
* command

정확한 Rust 타입과 channel 구현체는 repo 구조에 맞게 선택하세요.

## 7.2 Shell spawn policy

기본 shell 우선순위:

1. pwsh
2. powershell
3. cmd

사용 가능한 shell을 탐지하고 spawn하세요.
탐지 실패 시 명확한 error를 반환하세요.

## 7.3 Input

다음 IPC command를 구현하세요.

* writePane(paneId, data)
* optional: broadcastWrite(paneIds, data)

M0에서는 broadcastWrite를 구현하지 않아도 됩니다. 구현하지 않으면 interface/TODO만 남기고 명시하세요.

주의:

* stdin injection은 위험 기능이므로 명시적 paneId가 필요합니다.
* focused pane이 아닌 pane에 쓰는 경우도 API상 가능하되, 호출자가 paneId를 명시해야 합니다.
* watcher-triggered stdin injection은 M0에서 구현하지 마세요.

## 7.4 Output

PTY reader task는 output chunk를 읽고 다음 payload로 변환합니다.

```ts
type PtyOutputEvent = {
  workspaceId: WorkspaceId;
  paneId: PaneId;
  sessionId: SessionId;
  seq: number;
  data: string;
};
```

정책:

* active workspace output은 batch 후 frontend event로 emit
* inactive workspace output은 frontend event emit을 생략하거나 최소화하고 backend ring buffer에 저장
* workspace 재전환 시 pending output을 replay
* output ordering을 위해 seq counter 유지

---

# 8. Backpressure Strategy

다중 workspace와 다중 pane에서 동시에 대량 output이 발생할 수 있습니다.
Tauri event를 chunk마다 무제한 emit하지 마세요.

다음 조건을 만족하는 backpressure 전략을 설계하고 구현하세요.

## 8.1 Required behavior

* pane별 bounded buffer 또는 bounded channel
* active workspace output batching
* inactive workspace output buffering
* ring buffer 초과 시 정책 명시
* output seq counter 유지
* UI가 느려져도 backend reader가 무한 메모리 증가를 일으키지 않게 할 것

## 8.2 Suggested defaults

이 값은 강제값이 아니라 초기값입니다. repo 상황에 맞게 조정 가능하나, 변경 시 ADR에 이유를 기록하세요.

* target chunk size: 약 8KB
* max pending chunks per pane: 약 1024
* frontend emit batching interval: 16ms 또는 32ms
* ring buffer memory budget: pane당 제한

## 8.3 Drop policy

ring buffer 초과 시 다음 중 하나를 선택하고 기록하세요.

* oldest visual chunk drop
* inactive pane output summarization
* file spool fallback
* hard backpressure

M0에서는 oldest visual chunk drop 또는 bounded ring buffer를 우선 고려하세요.
단, “raw capture를 완전 보존한다”고 주장하려면 파일 spool 등 별도 구현이 있어야 합니다. 구현하지 않았다면 보존 보장이라고 쓰지 마세요.

---

# 9. xterm.js Integration

## 9.1 Responsibilities

xterm.js는 다음을 담당합니다.

* terminal screen rendering
* user keyboard input capture
* visual scrollback
* visual snapshot / restore

xterm.js는 다음을 담당하지 않습니다.

* PTY process ownership
* authoritative output capture
* workspace lifecycle
* backend ring buffer

## 9.2 Input path

```text
User keyboard input
→ xterm onData
→ Tauri command writePane(paneId, data)
→ backend PTY writer
```

필요한 경우 onBinary도 검토하세요.
IME/CJK 입력은 깨지지 않도록 주의하고, 문제가 있으면 known risk로 기록하세요.

## 9.3 Output path

```text
PTY reader
→ backend ring buffer
→ batched Tauri event
→ frontend xterm.write(data)
```

## 9.4 Snapshot path

```text
before workspace unmount
→ xterm serialize visual snapshot
→ frontend snapshot store
→ remount
→ restore snapshot
→ replay pending backend output
```

---

# 10. Persistence

config.json에 저장할 것:

* schemaVersion
* workspaces
* activeWorkspaceId
* pane layout tree
* pane cwd
* pane command
* optional UI preferences

저장하지 않을 것:

* running process state
* full scrollback
* raw output history
* shell runtime state
* command history

재시작 시:

* layout은 복원
* 각 pane에 새 shell spawn
* 이전 runtime output은 복원하지 않음
* sessionId는 새로 생성

Migration:

* legacy schema가 실제로 존재하지 않으면 임의 migration을 만들지 마세요.
* migration interface와 test stub만 만들어도 됩니다.
* 실제 migration을 구현했다면 sample legacy fixture를 포함하세요.

---

# 11. Performance Targets

M0 target:

* K=2 workspace × N=2 pane = 4 live PTY
* workspace switch p95 < 150ms
* 10분 soak 후 memory increase < 1.5x
* pane split/close가 즉시 반영될 것
* basic PTY input/output 정상 동작

M1 target, 설계 문서에만 포함:

* K=4 workspace × N=4 pane = 16 live PTY
* RSS < 1GB
* cached workspace switch p95 < 50ms
* 60분 soak 후 memory leak 없음
* K=2 workspace에서 동시 대량 output 중 active workspace 렌더링이 체감상 끊기지 않을 것

주의:

* M0에서 M1 target을 달성했다고 주장하지 마세요.
* benchmark를 실행하지 않았으면 수치를 추정하지 마세요.
* 수치를 측정했다면 측정 환경, 명령, 결과를 BENCHMARK.md에 기록하세요.

---

# 12. Security / Safety

stdin injection은 자동화 기능의 기반이지만 위험할 수 있습니다.

M0 정책:

* writePane은 명시적 paneId가 필요
* broadcastWrite는 구현하지 않거나 제한적으로 구현
* watcher-triggered stdin injection은 구현하지 않음
* 향후 automation은 allowlist 기반으로 설계
* destructive command 자동 실행은 기본 금지
* workspace 외부 pane에 임의 write하지 않음

---

# 13. Required Deliverables

다음을 반드시 산출하세요.

## 13.1 Documentation

* ARCHITECTURE.md
* ADR-001-backend-owned-session-registry.md
* ADR-002-workspace-keep-alive.md
* ADR-003-output-capture-vs-visual-snapshot.md
* ADR-004-backpressure-ring-buffer.md
* ADR-005-live-pty-memory-policy.md
* BENCHMARK.md

ARCHITECTURE.md에는 다음을 포함하세요.

* 전체 프로세스 구조
* frontend/backend 책임 분리
* PTY session ownership model
* workspace switch sequence
* pane split/close algorithm
* output flow
* input flow
* backpressure flow
* persistence model
* known risks

가능하면 Mermaid diagram을 포함하세요.

## 13.2 M0 Implementation

* Binary tree pane split/close
* Workspace CRUD
* Workspace switch
* PTY spawn per pane
* xterm rendering
* writePane IPC
* pty-output event
* config save/load
* inactive workspace keep-alive
* minimal ring buffer / backpressure

## 13.3 Tests

다음 테스트를 작성하세요.

* pane tree invariant tests
* splitPane tests
* closePane sibling promotion tests
* resizeSplit ratio clamp test
* workspace CRUD tests
* workspace switch state test
* config save/load test
* PTY spawn/write smoke test

테스트가 환경 문제로 실행되지 않으면 이유를 기록하세요.
실행하지 않은 테스트를 통과했다고 말하지 마세요.

## 13.4 Benchmark

간단한 benchmark script를 작성하세요.

최소 측정 항목:

* K workspace × N pane spawn
* workspace switch latency
* process memory
* simple output throughput
* soak test memory growth

M0 기본 benchmark:

* K=2, N=2
* short soak
* result written to BENCHMARK.md

---

# 14. Implementation Process

다음 순서로 작업하세요.

1. Repo 구조 조사
2. 기존 코드/스택 확인
3. Requirement audit
4. ARCHITECTURE.md 작성
5. ADR 작성
6. M0 구현
7. Unit test 작성
8. Smoke test 작성
9. Benchmark script 작성
10. build 실행
11. test 실행
12. benchmark 실행
13. 최종 보고

중간에 기술적 충돌이 발견되면:

* 임의로 숨기지 말 것
* ADR 또는 Known Risks에 기록할 것
* 더 안전한 대안을 선택할 것
* 선택 이유를 설명할 것

---

# 15. Completion Criteria

다음 조건을 만족해야 완료입니다.

* 앱이 build된다.
* 최소 하나의 workspace가 열린다.
* workspace 생성/전환/삭제가 동작한다.
* pane split/close가 동작한다.
* 각 pane에 PTY가 spawn된다.
* focused pane에 keyboard input이 전달된다.
* PTY output이 xterm에 표시된다.
* workspace 전환 시 PTY process가 종료되지 않는다.
* inactive workspace에서 돌아가던 process가 다시 전환했을 때 살아 있다.
* config.json save/load가 동작한다.
* tests가 실행된다.
* benchmark가 실행된다.
* 최종 보고에 실행 명령과 결과가 포함된다.

---

# 16. Do Not Do

다음을 하지 마세요.

* build/test 없이 완료 선언
* benchmark 없이 성능 수치 주장
* xterm serialize를 authoritative output capture라고 주장
* 256 live PTY를 M0 목표로 구현하려고 시도
* portable-pty가 지원하지 않는 ConPTY private flag를 무리하게 하드코딩
* inactive workspace output을 무제한 메모리에 쌓기
* frontend를 PTY session source of truth로 만들기
* watcher automation을 M0에서 무리하게 구현
* plugin architecture를 실제 구현한 것처럼 과장
* 실패한 테스트를 성공으로 보고
* “대략 됨”이라고 보고

---

# 17. Final Report Format

마지막 응답은 반드시 아래 형식으로 작성하세요.

## 구현 완료 항목

* ...

## 미구현 항목

* ...

## 설계상 주요 결정

* ...

## 변경한 파일

* ...

## 실행한 명령

```bash
...
```

## 테스트 결과

* ...

## Benchmark 결과

* ...

## Known Risks

* ...

## 다음 단계 제안

* ...
