# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **SPEC-PTY-FLOW-002**: ack 회계 단위 불일치(UTF-16 유닛 vs UTF-8 바이트)로 인한 non-ASCII 대량 출력 시 팬 출력 영구 정지 수정
  - 회계 단위 통일(R2/R5): `PtyOutputEvent.byteLen`(배너 포함 UTF-8 바이트, 백엔드 단일 원천)을 프론트가 반사 ack — UTF-16 코드 유닛 재산정 금지 (`src-tauri/src/output.rs`, `src/terms.ts`)
  - emitter 정지 안전밸브(R8): emitter 정지 + 10s 무ack 진전 시 회계 리셋·방출 재개 (`src-tauri/src/flow_state.rs`, `emitter_valve_fired` 카운터)
  - `TERMF_FLOW_STALL_TIMEOUT_MS` 환경변수로 밸브 타임아웃 오버라이드(테스트·bench 주입용)
  - `FlowStats`에 `valve_fired`/`emitter_valve_fired` 관측 필드 추가 (`flow_state.rs`, `src/types.ts`, `bin/bench.rs`)
  - 비ASCII(u8) 홍수 autotest 체크 6판정 추가 (`src/autotest.ts`, `report.flowOk` 집계)
  - **AC**: AC-1~AC-16 논리 16건 전부 PASS (cargo test 148 green, tsc exit 0, autotest ok·flowOk true, bench flow_ok=true·전 표본 emitter_valve_fired == 0)
  - **테스트**: `flow_tests.rs` `flow002_*` 재현-우선 회귀 테스트군 (단위 불일치 영구 정지 재현 → GREEN)
  - **문서**: ADR-014 v1.1.0 개정(회계 단위 명시 + emitter 밸브 + 잔여 누수 기명) + ARCHITECTURE.md §6 / DEVELOPMENT.md 갱신

### Added
- **SPEC-PTY-FLOW-001**: PTY 출력 흐름 제어 (ack-watermark flow control) + 워크스페이스 전환 출력 유실 수정
  - 프론트엔드 방향 흐름 제어: ack 기반 워터마크 게이트(R3)로 백엔드 방출을 제어
  - reader park(R4): ring 임계 초과 시 ConPTY 파이프 차오르게 하여 자식 write()를 블로킹 (활성 팬 데이터 무손실)
  - 정지 밸브(R6): 10s 무ack 시 자동 폴백 (죽은 프론트 보호)
  - parsedSeq 이원화(R10): 수신/파싱 시점 seq 분리로 워크스페이스 전환 내용 공백 수정(결함 2)
  - 회계 리셋(R15): 3지점 리셋으로 좌초 outstanding 제거
  - replay–emitter 경합 금지(R16): seq 되감김·중복 재방출 방지
  - 워터마크 상수: HIGH 128KiB / LOW 32KiB / RING_PAUSE_THRESHOLD 768KiB / FLOW_STALL_TIMEOUT 10s
  - 배치 ack(R9): 4KiB 배치 / 50ms idle 플러시로 IPC 최적화
  - 드레인(R11): 스냅샷 전 500ms 한도 대기
  - **AC**: AC-1~AC-15 전부 PASS (135 cargo tests, tsc 0, bench flow_ok=true, 실기기 autotest ok:true)
  - **테스트**: Rust 단위 테스트 21개 + autotest flood/switch 체크 + bench soak 시나리오
  - **문서**: ADR-014 + DEVELOPMENT.md/ARCHITECTURE.md 갱신
  - **수정**: R4 reader park 단위 불일치 버그 픽스(청크 수→바이트 비교, bench 재검증)

### Changed
- **이전 동작**: 활성 팬 홍수 시 프리즈 현상 → **이후 동작**: 자식 프로세스 자연 감속 (write 블로킹)
- **이전 동작**: 워크스페이스 전환 시 내용 공백 발생(결함 2) → **이후 동작**: parsedSeq 기반 replay로 공백 제거
- **이전 동작**: "[output overflow]" 배너가 활성 팬에서도 나타남 → **이후 동작**: 정지 밸브 폴백 경로에서만 표시 (활성 팬 정상 경로에서 제거)
