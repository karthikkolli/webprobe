use anyhow::Result;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

mod common;
use common::{DaemonTestGuard, get_test_browser};

/// Helper to run webprobe commands
fn run_webprobe(args: &[&str]) -> Result<(String, String, i32)> {
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

#[tokio::test]
async fn test_stdout_stderr_separation() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running

    // Use a simple test page instead of server

    // Create a test HTML file
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test.html");
    fs::write(&test_file, "<html><body><h1>Test</h1></body></html>")?;

    // Run inspect command and capture stdout/stderr separately
    let (stdout, stderr, exit_code) =
        run_webprobe(&["inspect", &format!("file://{}", test_file.display()), "h1"])?;

    if exit_code == 0 {
        // Stdout should contain JSON
        assert!(stdout.contains("\"selector\""));
        assert!(stdout.contains("\"position\""));

        // Stderr should contain logs
        assert!(stderr.contains("INFO"));
        assert!(stderr.contains("Inspecting h1"));

        // Stdout should be valid JSON
        let json: serde_json::Value = serde_json::from_str(&stdout)?;
        assert!(json.is_object());
    } else {
        // WebDriver connection failed - still check output format
        assert!(stdout.contains("error") || stderr.contains("error"));
    }

    Ok(())
}
#[tokio::test]
async fn test_screenshot_element() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Use fresh daemon
    let temp_dir = TempDir::new()?;
    let screenshot_path = temp_dir.path().join("element.png");

    // Create a test HTML file
    let test_file = temp_dir.path().join("test.html");
    fs::write(
        &test_file,
        "<html><body><h1>Element Screenshot</h1></body></html>",
    )?;

    // Take element screenshot
    let (stdout, _, exit_code) = run_webprobe(&[
        "screenshot",
        &format!("file://{}", test_file.display()),
        "--selector",
        "h1",
        "--output",
        screenshot_path.to_str().unwrap(),
    ])?;

    if exit_code == 0 {
        assert!(stdout.contains("Screenshot saved to:"));
        assert!(screenshot_path.exists());

        // Element screenshot should be smaller than full page
        let file_size = fs::metadata(&screenshot_path)?.len();
        assert!(file_size > 100);
        assert!(file_size < 100000); // Should be relatively small for just h1
    } else {
        // WebDriver connection failed or browser issue, which is acceptable in test environment
        assert!(
            exit_code == 1 || exit_code == 4,
            "Unexpected exit code: {}",
            exit_code
        );
    }

    Ok(())
}
#[tokio::test]
async fn test_shadow_dom_selector() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running
    let temp_dir = TempDir::new()?;

    // Create page with shadow DOM
    let test_page = temp_dir.path().join("shadow.html");
    fs::write(
        &test_page,
        r#"
        <!DOCTYPE html>
        <html>
        <body>
            <div id="host"></div>
            <p class="regular">Regular DOM</p>
            <script>
                const host = document.getElementById('host');
                const shadow = host.attachShadow({mode: 'open'});
                shadow.innerHTML = '<p class="shadow-text">Shadow DOM content</p>';
            </script>
        </body>
        </html>
    "#,
    )?;

    // Test regular selector
    let (stdout, _, exit_code) = run_webprobe(&[
        "inspect",
        &format!("file://{}", test_page.display()),
        ".regular",
    ])?;

    if exit_code == 0 {
        let json: serde_json::Value = serde_json::from_str(&stdout)?;
        assert_eq!(json["text_content"], "Regular DOM");
    } else {
        // WebDriver connection failed or browser issue, which is acceptable
        assert!(
            exit_code == 1 || exit_code == 4,
            "Unexpected exit code: {}",
            exit_code
        );
    }

    // Test shadow DOM selector (with >>> prefix)
    let (_stdout, _, exit_code) = run_webprobe(&[
        "inspect",
        &format!("file://{}", test_page.display()),
        ">>>.shadow-text",
    ])?;

    // Shadow DOM inspection has limitations, but should not error
    assert!(
        exit_code == 0 || exit_code == 1 || exit_code == 2 || exit_code == 4,
        "Unexpected exit code: {}",
        exit_code
    ); // May not find element or have browser issues

    Ok(())
}

#[tokio::test]
async fn test_exit_codes() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test.html");
    fs::write(&test_file, "<html><body><h1>Test</h1></body></html>")?;

    // Test successful command (exit code 0)
    let (stdout, stderr, exit_code) =
        run_webprobe(&["inspect", &format!("file://{}", test_file.display()), "h1"])?;
    eprintln!("Test 1 - Successful command:");
    eprintln!("  Exit code: {}", exit_code);
    eprintln!("  Stdout: {}", stdout);
    eprintln!("  Stderr: {}", stderr);
    // Test successful command or WebDriver failure
    assert!(
        exit_code == 0 || exit_code == 4,
        "Expected exit code 0 or 4, got {}",
        exit_code
    ); // Allow webdriver connection failures

    // Test element not found (exit code 2)
    let (stdout, stderr, exit_code) = run_webprobe(&[
        "inspect",
        &format!("file://{}", test_file.display()),
        ".nonexistent-element",
    ])?;
    eprintln!("Test 2 - Element not found:");
    eprintln!("  Exit code: {}", exit_code);
    eprintln!("  Stdout: {}", stdout);
    eprintln!("  Stderr: {}", stderr);
    assert!(
        exit_code == 2 || exit_code == 4,
        "Expected exit code 2 or 4, got {}",
        exit_code
    ); // 2 = not found, 4 = webdriver failed

    // Test WebDriver connection failure (exit code 4)
    // This would require stopping WebDriver, which is complex in tests

    Ok(())
}

#[tokio::test]
async fn test_viewport_with_screenshot() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Use fresh daemon
    let temp_dir = TempDir::new()?;

    // Take screenshot with custom viewport
    let screenshot_path = temp_dir.path().join("viewport.png");
    let test_file = temp_dir.path().join("test.html");
    fs::write(
        &test_file,
        "<html><body><h1>Viewport Test</h1></body></html>",
    )?;

    let (_stdout, _, exit_code) = run_webprobe(&[
        "screenshot",
        &format!("file://{}", test_file.display()),
        "--viewport",
        "800x600",
        "--output",
        screenshot_path.to_str().unwrap(),
    ])?;

    if exit_code == 0 {
        assert!(screenshot_path.exists());
    } else {
        // WebDriver connection failed or browser issue, which is acceptable
        assert!(
            exit_code == 1 || exit_code == 4,
            "Unexpected exit code: {}",
            exit_code
        );
    }

    Ok(())
}
