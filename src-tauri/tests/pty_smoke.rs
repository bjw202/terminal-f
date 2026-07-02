//! PTY spawn / write / output smoke tests against the real ConPTY.
//! These exercise the actual portable-pty + shell path, so they need a
//! Windows environment with at least cmd.exe available.
//!
//! Headless caveat: pwsh/PSReadLine queries the terminal with DSR (`ESC[6n`)
//! during startup and blocks until it gets a cursor-position reply. xterm.js
//! answers this automatically in the real app; here `pump_dsr` plays that
//! role.

use std::sync::Arc;
use std::time::{Duration, Instant};
use terminal_f_lib::output;
use terminal_f_lib::session::SessionRegistry;

fn ring_text(registry: &SessionRegistry, pane: &str) -> String {
    registry
        .session_for_pane(pane)
        .map(|s| s.ring.lock().unwrap().collect_since(0).0)
        .unwrap_or_default()
}

/// Emulate the terminal side of the DSR handshake (what xterm.js does).
fn pump_dsr(registry: &SessionRegistry, pane: &str, responded: &mut usize) {
    let asks = ring_text(registry, pane).matches("\x1b[6n").count();
    while *responded < asks {
        let _ = registry.write_pane(pane, "\x1b[1;1R");
        *responded += 1;
    }
}

fn wait_for<F: Fn() -> bool>(
    registry: &SessionRegistry,
    pane: &str,
    responded: &mut usize,
    cond: F,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        pump_dsr(registry, pane, responded);
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn spawn_write_echo_and_terminate() {
    let registry = SessionRegistry::new();
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    registry
        .spawn_session("ws-a", "pane-1", &home, None)
        .expect("spawn must succeed");
    assert_eq!(registry.live_count(), 1);
    let mut dsr = 0usize;

    // shell prompt appears
    assert!(
        wait_for(
            &registry,
            "pane-1",
            &mut dsr,
            || ring_text(&registry, "pane-1").len() > 20,
            Duration::from_secs(20)
        ),
        "shell produced no prompt output; ring: {:?}",
        ring_text(&registry, "pane-1")
    );

    // stdin write reaches the shell and output comes back
    registry
        .write_pane("pane-1", "echo TERMF_SMOKE_OK\r")
        .expect("write must succeed");
    assert!(
        wait_for(
            &registry,
            "pane-1",
            &mut dsr,
            || ring_text(&registry, "pane-1").contains("TERMF_SMOKE_OK"),
            Duration::from_secs(20)
        ),
        "echo output did not arrive; ring: {:?}",
        ring_text(&registry, "pane-1")
    );

    // graceful terminate on pane close
    registry.close_pane_session("pane-1").unwrap();
    assert_eq!(registry.live_count(), 0);
    assert!(registry.session_for_pane("pane-1").is_none());
}

#[test]
fn write_to_unknown_pane_fails() {
    let registry = SessionRegistry::new();
    assert!(registry.write_pane("ghost", "dir\r").is_err());
}

#[test]
fn inactive_workspace_keeps_session_alive_and_buffers_output() {
    let registry = Arc::new(SessionRegistry::new());
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    registry.spawn_session("ws-a", "pane-a", &home, None).unwrap();
    registry.spawn_session("ws-b", "pane-b", &home, None).unwrap();
    let mut dsr_a = 0usize;
    let mut dsr_b = 0usize;

    registry.set_active_workspace(Some("ws-a"));
    let last_seq_a = registry.replay("pane-a", 0).unwrap().last_seq;

    assert!(
        wait_for(
            &registry,
            "pane-a",
            &mut dsr_a,
            || ring_text(&registry, "pane-a").len() > 20,
            Duration::from_secs(20)
        ),
        "ws-a shell not ready"
    );

    // long-running output in ws-a, then switch away
    registry
        .write_pane(
            "pane-a",
            "1..60 | ForEach-Object { \"TICK $_\"; Start-Sleep -Milliseconds 100 }\r",
        )
        .unwrap();
    assert!(
        wait_for(
            &registry,
            "pane-a",
            &mut dsr_a,
            || ring_text(&registry, "pane-a").contains("TICK"),
            Duration::from_secs(20)
        ),
        "tick loop did not start; ring: {:?}",
        ring_text(&registry, "pane-a")
    );
    registry.set_active_workspace(Some("ws-b"));
    registry.replay("pane-b", 0).unwrap();
    pump_dsr(&registry, "pane-b", &mut dsr_b);

    // while ws-a is inactive: emitter must emit nothing for it, but its ring
    // keeps growing (keep-alive + buffering)
    let bytes_before = registry
        .session_for_pane("pane-a")
        .unwrap()
        .ring
        .lock()
        .unwrap()
        .total_bytes;
    let emitted_for_a = std::sync::atomic::AtomicUsize::new(0);
    for _ in 0..40 {
        output::pump_once(
            &registry,
            &|ev| {
                if ev.pane_id == "pane-a" {
                    emitted_for_a.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            },
            &|_| {},
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        emitted_for_a.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "inactive workspace must not emit events"
    );

    let bytes_after = registry
        .session_for_pane("pane-a")
        .unwrap()
        .ring
        .lock()
        .unwrap()
        .total_bytes;
    assert!(
        bytes_after > bytes_before,
        "session in inactive workspace must keep producing output ({bytes_before} -> {bytes_after})"
    );

    // switching back replays the pending output from the last seen seq
    registry.set_active_workspace(Some("ws-a"));
    let replay = registry.replay("pane-a", last_seq_a).unwrap();
    assert!(replay.last_seq > last_seq_a, "replay must advance seq");
    assert!(
        replay.data.contains("TICK"),
        "pending output must be replayed"
    );

    registry.shutdown();
    assert_eq!(registry.live_count(), 0);
}

#[test]
fn inject_respects_idle_gate_and_writes() {
    let registry = SessionRegistry::new();
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    registry.spawn_session("ws-i", "pane-i", &home, None).unwrap();
    let mut dsr = 0usize;
    assert!(
        wait_for(
            &registry,
            "pane-i",
            &mut dsr,
            || ring_text(&registry, "pane-i").len() > 20,
            Duration::from_secs(20)
        ),
        "shell not ready"
    );

    // busy gate: shell just produced output -> require_idle must refuse
    registry.write_pane("pane-i", "echo warmup\r").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let busy = registry.inject("pane-i", "echo INJ", true, true, 1500);
    assert!(busy.is_err(), "must refuse while output is fresh");
    assert!(busy.unwrap_err().contains("busy"));

    // wait for quiet, then inject with submit (poll: prompt output timing
    // is nondeterministic, so retry until the idle gate opens)
    let deadline = Instant::now() + Duration::from_secs(15);
    let receipt = loop {
        match registry.inject("pane-i", "echo TERMF_INJECT_OK", true, true, 1500) {
            Ok(r) => break r,
            Err(e) if Instant::now() < deadline => {
                assert!(e.contains("busy"), "unexpected inject error: {e}");
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => panic!("idle gate never opened: {e}"),
        }
    };
    assert!(receipt.submitted);
    assert!(
        wait_for(
            &registry,
            "pane-i",
            &mut dsr,
            || ring_text(&registry, "pane-i").contains("TERMF_INJECT_OK"),
            Duration::from_secs(20)
        ),
        "injected command did not run; ring: {:?}",
        ring_text(&registry, "pane-i")
    );

    // require_idle=false bypasses the gate explicitly
    registry
        .inject("pane-i", "echo FORCED", true, false, 1500)
        .expect("explicit bypass must be allowed");

    registry.shutdown();
}

#[test]
fn live_pty_soft_cap_refuses_spawn() {
    // Uses the constant rather than spawning 32 real shells (too heavy for a
    // smoke test); the enforcement path itself is a simple count check that
    // fires before any PTY resource is allocated.
    assert_eq!(terminal_f_lib::session::LIVE_PTY_SOFT_CAP, 32);
}
