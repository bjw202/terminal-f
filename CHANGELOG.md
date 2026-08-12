# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
