use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

mod test_server;
use test_server::ensure_test_server;

/// Simple test to verify daemon can start and handle basic operations
#[tokio::test]
async fn test_daemon_basic_operation() {
    // Start test server
    let server = ensure_test_server().await;

    // Stop any existing daemon first using proper shutdown
    let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "stop"])
        .output();

    sleep(Duration::from_secs(1)).await;

    // Start daemon with Chrome browser
    let mut daemon_process = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "run", "--browser", "chrome"])
        .spawn()
        .expect("Failed to start daemon");

    // Give daemon time to start
    sleep(Duration::from_secs(3)).await;

    // Test inspection using daemon - use one-shot operation
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["inspect", &server.base_url, "h1"])
        .output()
        .expect("Failed to run inspect command");

    // Check if command succeeded
    if !output.status.success() {
        eprintln!("Inspect failed!");
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    }

    // Properly stop the daemon (this should trigger our signal handler)
    let stop_result = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "stop"])
        .output();

    // If stop command failed, kill the process as fallback
    if stop_result.is_err() || !stop_result.unwrap().status.success() {
        daemon_process.kill().ok();
        sleep(Duration::from_millis(500)).await;
    }

    // Wait for daemon to fully shut down
    let _ = daemon_process.wait();

    assert!(output.status.success(), "Inspect command should succeed");

    // Verify output contains h1 element info
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("selector") || stdout.contains("position") || stdout.contains("h1"),
        "Output should contain element information, got: {}",
        stdout
    );
}
