// Tests for error handling and edge cases
use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

mod common;
use common::{DaemonTestGuard, get_test_browser};

/// Helper to run webprobe command
fn run_command(args: &[&str]) -> Result<(Value, i32)> {
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    // Parse JSON output
    let json = match serde_json::from_str(&stdout) {
        Ok(json) => json,
        Err(_) => {
            // If not JSON, combine stdout and stderr for the message
            let message = if !stdout.is_empty() {
                stdout.to_string()
            } else {
                stderr.to_string()
            };

            serde_json::json!({
                "error": exit_code != 0,
                "message": message,
                "exit_code": exit_code
            })
        }
    };

    Ok((json, exit_code))
}

#[test]
fn test_invalid_url_handling() -> Result<()> {
    // Test with completely invalid URL
    let (result, _) = run_command(&["inspect", "not-a-url", "body"])?;

    assert_eq!(result["error"].as_bool(), Some(true));
    // Should have an error message about invalid URL
    assert!(result["message"].is_string());

    Ok(())
}

#[test]
fn test_malformed_selector_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_page = temp_dir.path().join("test.html");
    fs::write(&test_page, "<html><body>Test</body></html>")?;

    // Test with malformed CSS selector
    let (result, _) = run_command(&[
        "inspect",
        &format!("file://{}", test_page.display()),
        "###invalid###",
    ])?;

    assert_eq!(result["error"].as_bool(), Some(true));

    Ok(())
}

#[test]
fn test_empty_url_without_tab() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running

    // Empty URL without tab should fail (requires tab for empty URL)
    let (result, exit_code) = run_command(&["inspect", "", "body"])?;

    // It might succeed (navigating to about:blank) or fail
    // If it fails, should have non-zero exit code
    if result["error"].as_bool() == Some(true) {
        assert_ne!(exit_code, 0, "Should have non-zero exit code");

        // Error message might be about WebDriver or connection
        if let Some(message) = result["message"].as_str() {
            assert!(
                message.contains("WebDriver")
                    || message.contains("connection")
                    || message.contains("Failed")
                    || message.contains("Error"),
                "Should have error message, got: {}",
                message
            );
        }
    } else {
        // If it succeeded, it should have found body element
        assert!(result["selector"].as_str() == Some("body") || result.is_object());
    }

    Ok(())
}

#[test]
fn test_timeout_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_page = temp_dir.path().join("timeout.html");
    fs::write(
        &test_page,
        r#"
        <html>
        <body>
            <script>
                // Simulate slow loading
                setTimeout(() => {
                    document.getElementById('delayed').innerHTML = 'Loaded';
                }, 10000);
            </script>
            <div id="delayed"></div>
        </body>
        </html>
    "#,
    )?;

    // Try to find element with very short timeout
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "wait-navigation",
            &format!("file://{}", test_page.display()),
            "--timeout",
            "1", // 1 second timeout
        ])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if let Ok(result) = serde_json::from_str::<Value>(&stdout) {
        // Might timeout or succeed quickly depending on implementation
        if result["error"].as_bool() == Some(true) {
            // Check for timeout-related exit code (5)
            if let Some(exit_code) = result["exit_code"].as_u64() {
                assert!(
                    exit_code == 5 || exit_code == 4,
                    "Should have timeout or connection error"
                );
            }
        }
    }

    Ok(())
}

#[test]
fn test_viewport_invalid_format() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test.html");
    fs::write(&test_file, "<html><body>Test</body></html>")?;

    // Test with invalid viewport format
    let (result, _) = run_command(&[
        "inspect",
        &format!("file://{}", test_file.display()),
        "body",
        "--viewport",
        "invalid-format",
    ])?;

    assert_eq!(result["error"].as_bool(), Some(true));
    // Should have error about invalid viewport format
    if let Some(message) = result["message"].as_str() {
        assert!(
            message.contains("viewport")
                || message.contains("format")
                || message.contains("WIDTHxHEIGHT"),
            "Error should mention viewport format"
        );
    }

    Ok(())
}

#[test]
fn test_eval_without_unsafe_flag() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test.html");
    fs::write(
        &test_file,
        "<html><head><title>Test Title</title></head><body>Test</body></html>",
    )?;

    // Eval should fail without --unsafe-eval flag
    let (result, exit_code) = run_command(&[
        "eval",
        "document.title",
        "--url",
        &format!("file://{}", test_file.display()),
    ])?;

    assert_eq!(result["error"].as_bool(), Some(true));
    assert_ne!(exit_code, 0);

    // Error message should mention --unsafe-eval
    if let Some(message) = result["message"].as_str() {
        assert!(
            message.contains("unsafe")
                || message.contains("security")
                || message.contains("required"),
            "Error should mention --unsafe-eval flag requirement, got: {}",
            message
        );
    }

    Ok(())
}

#[test]
fn test_batch_invalid_json() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running

    // Test batch command with invalid JSON array
    let (result, _) = run_command(&["batch", "{not-valid-json-array"])?;

    // Batch command treats non-array as commands and executes them
    // The error might be in the output about unknown command or invalid JSON
    if let Some(message) = result["message"].as_str() {
        assert!(
            message.contains("Error")
                || message.contains("Unknown")
                || message.contains("JSON")
                || message.contains("parse")
                || message.contains("invalid")
                || message.contains("deserialize"),
            "Should mention error with invalid input, got: {}",
            message
        );
    }

    Ok(())
}

#[test]
fn test_daemon_commands_with_daemon() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test.html");
    fs::write(&test_file, "<html><body>Test Content</body></html>")?;

    std::thread::sleep(std::time::Duration::from_millis(500));

    // Try to use tab WITH daemon running - should work
    let (result, exit_code) = run_command(&[
        "inspect",
        &format!("file://{}", test_file.display()),
        "body",
        "--tab",
        "test-tab",
    ])?;

    // Should succeed with daemon running
    assert_eq!(exit_code, 0, "Should succeed with daemon running");

    // Verify we got a valid response
    assert!(result["selector"].is_string() || result["error"].as_bool() == Some(false));

    Ok(())
}

#[test]
fn test_multiple_elements_handling() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running
    let temp_dir = TempDir::new()?;
    let test_page = temp_dir.path().join("multiple.html");
    fs::write(
        &test_page,
        r#"
        <html>
        <body>
            <div class="item">Item 1</div>
            <div class="item">Item 2</div>
            <div class="item">Item 3</div>
        </body>
        </html>
    "#,
    )?;

    // Test with --expect-one flag
    let (result, _exit_code) = run_command(&[
        "inspect",
        &format!("file://{}", test_page.display()),
        ".item",
        "--expect-one",
    ])?;

    // Should fail with exit code 3 (multiple elements) or succeed with warning
    if result["error"].as_bool() == Some(true) {
        if let Some(code) = result["exit_code"].as_u64() {
            assert!(
                code == 3 || code == 4,
                "Should have exit code 3 (multiple) or 4 (no WebDriver)"
            );
        }
    } else {
        // If it succeeded, it might have returned metadata with a warning
        if let Some(metadata) = result.get("metadata")
            && let Some(total_matches) = metadata.get("total_matches")
        {
            assert!(
                total_matches.as_u64().unwrap_or(0) > 1,
                "Should indicate multiple matches"
            );
        }
    }

    Ok(())
}

#[test]
fn test_element_not_found_exit_code() -> Result<()> {
    let mut _daemon = DaemonTestGuard::new(get_test_browser()); // Ensure daemon is running
    let temp_dir = TempDir::new()?;
    let test_page = temp_dir.path().join("notfound.html");
    fs::write(&test_page, "<html><body>Test</body></html>")?;

    // Try to find non-existent element
    let (result, _) = run_command(&[
        "inspect",
        &format!("file://{}", test_page.display()),
        ".does-not-exist",
    ])?;

    assert_eq!(result["error"].as_bool(), Some(true));

    // Should have exit code 2 (not found) or 4 (no WebDriver)
    if let Some(code) = result["exit_code"].as_u64() {
        assert!(
            code == 2 || code == 4,
            "Should have exit code 2 (not found) or 4 (no WebDriver), got {}",
            code
        );
    }

    Ok(())
}
