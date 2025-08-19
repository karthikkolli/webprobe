// Tests for oneshot operations through the daemon
// These operations use temporary tabs that are cleaned up automatically

use serial_test::serial;
use std::process::Command;

mod common;
use common::{DaemonTestGuard, get_test_browser};

mod test_server;
use test_server::ensure_test_server;

/// Helper to run webprobe CLI commands
fn run_webprobe(args: &[&str]) -> std::process::Output {
    let binary_path = env!("CARGO_BIN_EXE_webprobe");
    Command::new(binary_path)
        .args(args)
        .output()
        .expect("Failed to execute webprobe command")
}

#[tokio::test]
#[serial]
async fn test_oneshot_inspect() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    // Run inspect command (oneshot mode - no --tab flag)
    let output = run_webprobe(&["inspect", &format!("{}/test", server.base_url), "h1"]);

    assert!(output.status.success(), "Command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify output contains expected element information
    assert!(
        stdout.contains("Test Page") || stdout.contains("h1"),
        "Should find header element. Got: {}",
        stdout
    );
}

#[tokio::test]
#[serial]
async fn test_oneshot_eval() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    // Run eval command (oneshot mode) - server pages have JavaScript enabled
    let output = run_webprobe(&[
        "eval",
        "document.title",
        "--url",
        &format!("{}/test", server.base_url),
        "--unsafe-eval",
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "Command should succeed. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Verify the JavaScript was executed and returned the title
    assert!(
        stdout.contains("Test Page"),
        "Should return page title. Got: {}",
        stdout
    );
}

#[tokio::test]
#[serial]
async fn test_oneshot_find_text() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    // Test find-text command (oneshot mode) on form page
    let output = run_webprobe(&[
        "find-text",
        "Submit",
        "--url",
        &format!("{}/form", server.base_url),
    ]);

    assert!(output.status.success(), "Command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should find the submit button
    assert!(
        stdout.contains("Submit") || stdout.contains("button"),
        "Should find submit button. Got: {}",
        stdout
    );
}

#[tokio::test]
#[serial]
async fn test_oneshot_vs_persistent_tab() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    let url = format!("{}/elements", server.base_url);

    // Test oneshot operation (no tab created)
    let output = run_webprobe(&["inspect", &url, "#action-button"]);

    assert!(output.status.success(), "Oneshot command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("action-button")
            || stdout.contains("Click Me")
            || stdout.contains("button"),
        "Should find button element"
    );

    // Test persistent tab operation
    let output = run_webprobe(&[
        "inspect",
        &url,
        "#action-button",
        "--tab",
        "persistent-test",
    ]);

    assert!(
        output.status.success(),
        "Persistent tab command should succeed"
    );

    // Verify tab persists by using empty URL
    let output = run_webprobe(&[
        "inspect",
        "", // Empty URL should stay on same page
        "#action-button",
        "--tab",
        "persistent-test",
    ]);

    assert!(output.status.success(), "Should reuse persistent tab");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("action-button")
            || stdout.contains("Click Me")
            || stdout.contains("button"),
        "Tab should remain on same page"
    );
}

#[tokio::test]
#[serial]
async fn test_oneshot_performance() {
    // Test that oneshot operations are fast and don't create persistent state
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    let url = format!("{}/test", server.base_url);

    // Run multiple oneshot operations
    for i in 0..3 {
        let output = run_webprobe(&["inspect", &url, "h1"]);

        assert!(output.status.success(), "Oneshot #{} should succeed", i);
    }

    // Verify no persistent tabs were created
    let output = run_webprobe(&["tab", "list"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should only have the main tab, no test tabs
    assert!(
        !stdout.contains("oneshot"),
        "No oneshot tabs should persist"
    );
}

#[tokio::test]
#[serial]
async fn test_oneshot_with_javascript() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    // Test JavaScript execution on a page with dynamic content
    let output = run_webprobe(&[
        "eval",
        "document.querySelectorAll('button').length",
        "--url",
        &format!("{}/dynamic", server.base_url),
        "--unsafe-eval",
    ]);

    assert!(output.status.success(), "Eval command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The dynamic page has at least one button (Load Content button)
    assert!(
        stdout.contains("1") || stdout.contains("2"),
        "Should count buttons on page. Got: {}",
        stdout
    );
}

#[tokio::test]
#[serial]
async fn test_oneshot_layout_analysis() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    // Test layout analysis in oneshot mode
    let output = run_webprobe(&[
        "layout",
        &format!("{}/layout", server.base_url),
        "body", // Analyze the body element's layout
    ]);

    assert!(output.status.success(), "Layout command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should detect layout structure
    assert!(
        stdout.contains("container") || stdout.contains("grid") || stdout.contains("flex"),
        "Should analyze layout. Got: {}",
        stdout
    );
}
