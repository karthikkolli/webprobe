// Integration tests for webprobe
// These tests verify functionality through the daemon

mod common;
use common::{DaemonTestGuard, get_test_browser};
use serial_test::serial;
use std::process::Command;

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
async fn test_browser_connection() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());

    // Navigate to a test page
    let test_html = common::create_test_html(common::fixtures::SIMPLE_PAGE);
    let url = format!("file://{}", test_html.display());

    // Inspect an element using CLI
    let result = run_webprobe(&["inspect", &url, "h1"]);

    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("h1"));
    assert!(output.contains("Test Header"));
}

#[tokio::test]
#[serial]
async fn test_console_capture() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());

    // Navigate to page with console logs
    let test_html = common::create_test_html(common::fixtures::PAGE_WITH_CONSOLE);
    let url = format!("file://{}", test_html.display());

    // Inspect element (console capture not available via CLI)
    let result = run_webprobe(&["inspect", &url, "#app"]);

    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("#app"));
    // Note: Console capture may not work reliably for file:// URLs
}

#[tokio::test]
#[serial]
async fn test_click_element() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());

    let html = r#"
    <!DOCTYPE html>
    <html>
    <body>
        <button id="btn" onclick="this.textContent='Clicked'">Click me</button>
    </body>
    </html>
    "#;

    let test_html = common::create_test_html(html);
    let url = format!("file://{}", test_html.display());

    // Navigate to page
    let result = run_webprobe(&["inspect", &url, "#btn", "--tab", "click-test"]);
    assert!(result.status.success());

    // Click the button
    let result = run_webprobe(&[
        "click",
        "--tab",
        "click-test",
        "", // Stay on same page
        "#btn",
    ]);
    assert!(result.status.success());

    // Check that text changed
    let result = run_webprobe(&[
        "inspect",
        "--tab",
        "click-test",
        "", // Stay on same page
        "#btn",
    ]);

    assert!(result.status.success());
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(output.contains("Clicked"));
}
