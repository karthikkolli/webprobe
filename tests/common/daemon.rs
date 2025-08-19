// Legacy daemon management functions
// These are kept for tests that specifically test daemon lifecycle
// For normal tests, use DaemonTestGuard from test_guard.rs

use std::process::Command;
use std::thread;
use std::time::Duration;

/// Stop the daemon if it's running
pub fn stop_daemon() {
    let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "stop"])
        .output();

    // Give it time to stop
    thread::sleep(Duration::from_millis(500));
}

// Note: Old DaemonGuard has been removed - use DaemonTestGuard from test_guard.rs
// DaemonTestGuard provides:
// - Automatic cleanup on drop (even on panic)
// - Test isolation (fails if daemon already running)
// - Profile management and cleanup
// - WebDriver/browser process cleanup
