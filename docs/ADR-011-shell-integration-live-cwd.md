# ADR-011: 셸 통합 기반 라이브 cwd 추적

상태: 채택 (UX③, 2026-07-06)

## 맥락

pane를 split하면 새 pane는 원본 leaf의 `cwd`(생성 시점 경로)를 상속한다.
그러나 사용자가 셸에서 `cd`로 이동한 **현재 경로**는 백엔드가 알 수 없어,
split한 새 pane이 엉뚱한(처음 열렸던) 디렉터리에서 열린다.

Windows 특성상 이 문제는 자식 프로세스 CWD 조회로 풀 수 없다:
**PowerShell의 `Set-Location`은 프로세스 CWD를 바꾸지 않는다**(런스페이스별
위치를 내부 관리). 따라서 PEB를 읽어도 pwsh의 현재 위치를 알 수 없다.

## 결정

Windows Terminal과 동일하게 **셸 통합(OSC 이스케이프 시퀀스)** 으로 해결한다.

1. **파싱(백엔드)**: 세션 reader 스레드가 출력 스트림에서
   `OSC 9;9;<path>`(ConEmu/WT 규약)와 `OSC 7;file://host/<path>`(범용)를
   파싱해 `PtySession.last_cwd`를 갱신한다. 종료자는 BEL(`0x07`) 또는
   ST(`ESC \`). 시퀀스가 읽기 청크 경계에 걸릴 수 있으므로 reader에 조립
   버퍼(`cwd_buf`)를 두고, 완전한 시퀀스만 소비하며 미완성 꼬리는 다음
   청크까지 보관한다(8KiB 초과 시 폐기). 파서는 순수 함수 `scan_cwd`로
   단위 테스트된다(완전/청크분할/OSC7/비ASCII/무관 OSC).

2. **split_pane**: `registry.pane_live_cwd(pane)`가 있으면(그리고 실제
   디렉터리이면) 그것을, 없으면 현행대로 leaf의 cwd를 사용한다. 셸 통합이
   없거나 아직 보고 전이면 자연스럽게 기존 동작으로 폴백한다.

3. **표시 억제(프론트)**: 같은 출력 스트림이 xterm으로도 가므로 OSC 9;9가
   화면에 찌꺼기로 보이지 않도록 `registerOscHandler(9, data => data.startsWith("9;"))`
   로 삼킨다. 다른 OSC 9 용도(9;4 진행률 등)는 통과시킨다.

4. **셸 통합 설치(opt-in)**: 팔레트 "Shell: Enable live directory tracking"
   → `$PROFILE`에 프롬프트 래퍼 스니펫을 추가(shellint.rs `cwd_snippet`).
   래퍼는 기존 prompt를 `__termf_orig_prompt`로 보존해 체이닝하므로
   oh-my-posh/starship와 공존한다. **스니펫 전문을 보여주고 확인**받은 뒤
   기록하며(사용자 파일 수정), fence 주석으로 감싸 멱등 설치/제거가 된다.
   [[ADR-010]]의 멀티라인 설치와 같은 shellint 인프라를 공유한다.

   **중요(실기기 교훈)**: OSC는 반드시 **프롬프트 함수의 반환 문자열에
   prepend**해야 한다(`return $osc + $base`). 프롬프트 함수 안에서
   `[Console]::Write`/`Write-Host`로 직접 출력하면 **PSReadLine 렌더링
   경로에서 신뢰성 있게 PTY로 나가지 않아** 방출이 실패한다. Windows
   Terminal·VS Code의 PowerShell 셸 통합도 반환-문자열 방식을 쓴다. autotest는
   파서(직접 OSC 주입)뿐 아니라 **프롬프트가 OSC를 반환하는 실제 방출 경로**
   까지 검증해야 이 부류의 버그를 잡는다(`cwdPromptEmit`).

   **업그레이드(in-place)**: 설치 커맨드는 `with_block(without_block(...))`로
   **기존(구버전) 블록을 제거 후 최신 블록을 다시 추가**한다. 상태 조회는
   `up_to_date`(= 현재 스니펫이 프로필에 그대로 존재)를 반환하고, 프론트는
   `installed && !up_to_date`이면 "이미 설치됨" 대신 **[Update] 확인창**을
   띄워 스니펫이 바뀌어도 사용자가 갱신할 수 있게 한다. (초기 버그: 프론트가
   `installed`이면 무조건 안내창만 띄우고 install을 호출하지 않아 구버전
   블록이 갱신되지 않았다.)

## 검토한 대안

- **win32-input-mode / PEB 읽기**: pwsh는 프로세스 CWD를 안 바꾸므로 무효.
- **프론트에서 OSC 파싱**: 백엔드가 소유 원칙(ADR-001) 위반이고, split은
  백엔드 커맨드라 백엔드가 알아야 한다.
- **cmd.exe 지원**: cmd는 `cd`가 프로세스 CWD를 바꾸므로 향후 PEB 폴백
  검토 가능. v1은 pwsh만 지원(문서화).

## 트레이드오프

- 프롬프트마다 OSC 한 줄이 출력에 섞인다(백엔드 파싱 + 프론트 억제로
  무해). 성능 영향은 chunk당 O(n) ESC 스캔으로 무시할 수준.
- 프로필 수정은 사용자 환경 변경 → 반드시 opt-in + 전문 표시 + 확인 +
  새 pane 필요(프로필은 셸 시작 때만 로드).
- 청크 경계에 걸친 시퀀스가 조립 버퍼로 처리되지만, 8KiB를 넘는 미완성
  시퀀스는 폐기한다(정상 경로에선 발생 불가).

## 향후 계획

- 워크스페이스 전환/앱 종료 시 `last_cwd`를 leaf.cwd에 반영해 config에
  저장 → 재시작 후에도 마지막 위치에서 시작.
- 실행 중 명령/종료 상태(OSC 133) 파싱으로 확장(pane 헤더에 실행 중 명령
  표시, 장시간 명령 완료 알림).
