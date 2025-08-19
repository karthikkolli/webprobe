// Integration tests for daemon workflows
// Tests real user scenarios with daemon, tabs, and command interactions
//
// IMPORTANT: These tests MUST be run with --test-threads=1 or use the #[serial]
// attribute because they share a single daemon instance. Running in parallel will
// cause failures as tests interfere with each other's daemon state.
//
// NOTE: There is a known issue where the daemon may crash when switching between
// Chrome and Firefox browsers within the same test session. Tests should be run
// with TEST_BROWSER=chrome or TEST_BROWSER=firefox to test each browser separately.

mod test_server;
use test_server::ensure_test_server;

use serial_test::serial;
use std::process::{Command, ExitStatus};
use std::time::Duration;
use tokio::time::sleep;

/// A struct to hold the complete result of a command execution
#[derive(Debug)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    /// Check if command succeeded
    pub fn is_success(&self) -> bool {
        self.status.success()
    }

    /// Get exit code if available
    pub fn exit_code(&self) -> Option<i32> {
        self.status.code()
    }

    /// Check if stdout contains a string
    pub fn stdout_contains(&self, text: &str) -> bool {
        self.stdout.contains(text)
    }

    /// Check if stderr contains a string
    pub fn stderr_contains(&self, text: &str) -> bool {
        self.stderr.contains(text)
    }
}

/// Helper to run webprobe CLI commands
/// Uses Cargo's env macro to find the correct binary path
fn run_webprobe(args: &[&str]) -> CommandOutput {
    // This works for both `cargo test` and `cargo test --release`
    let binary_path = env!("CARGO_BIN_EXE_webprobe");

    let output = Command::new(binary_path)
        .args(args)
        .output()
        .expect("Failed to execute webprobe command");

    CommandOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

/// Ensure daemon is stopped before test
async fn ensure_daemon_stopped() {
    let _result = run_webprobe(&["daemon", "stop"]);
    // Don't care if it fails (daemon might not be running)
    sleep(Duration::from_millis(500)).await;

    // Verify daemon is actually stopped
    let status = run_webprobe(&["daemon", "status"]);
    assert!(
        status.stdout_contains("Daemon is not running"),
        "Daemon should be stopped. stdout: {}, stderr: {}",
        status.stdout,
        status.stderr
    );
}

/// Helper function to ensure test profile exists
fn ensure_test_profile(profile_name: &str, browser: &str) {
    let _ = run_webprobe(&["profile", "destroy", profile_name, "--force"]);
    let result = run_webprobe(&["profile", "create", profile_name, "--browser", browser]);
    assert!(
        result.is_success(),
        "Failed to create profile '{}': {}",
        profile_name,
        result.stderr
    );
}

/// Start daemon with specific browser and wait for it to be ready
async fn start_daemon_with_browser(browser: &str) {
    let result = run_webprobe(&["daemon", "start", "--browser", browser]);
    assert!(
        result.is_success(),
        "Failed to start daemon with browser {}: stderr={}, stdout={}",
        browser,
        result.stderr,
        result.stdout
    );

    // Wait for daemon to be ready with longer timeout
    for i in 0..20 {
        sleep(Duration::from_millis(500)).await;
        let status = run_webprobe(&["daemon", "status"]);
        if status.stdout_contains("Daemon is running") {
            println!("Daemon started successfully after {} attempts", i + 1);
            return;
        }
        if i == 19 {
            panic!(
                "Daemon failed to start after 10 seconds\nstdout: {}\nstderr: {}",
                status.stdout, status.stderr
            );
        }
    }
}

/// Get the list of browsers to test
fn get_test_browsers() -> Vec<&'static str> {
    // Check environment variable to allow testing specific browser
    if let Ok(browser) = std::env::var("TEST_BROWSER") {
        vec![Box::leak(browser.into_boxed_str())]
    } else {
        // Test both by default
        vec!["chrome", "firefox"]
    }
}

#[tokio::test]
#[serial]
async fn test_daemon_tab_persistence() {
    for browser in get_test_browsers() {
        // Stop and restart daemon for each browser
        ensure_daemon_stopped().await;
        start_daemon_with_browser(browser).await;
        println!("\n=== Testing tab persistence with {} ===", browser);

        let server = ensure_test_server().await;
        let base_url = server.base_url.clone();
        let test_url = format!("{}/elements", base_url);

        // Create test profile for this test
        let profile_name = "tab-persistence-profile";
        ensure_test_profile(profile_name, browser);

        // Create a tab and navigate (no --browser needed, daemon knows which browser to use)
        let result = run_webprobe(&[
            "inspect",
            &test_url,
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "test-tab",
        ]);
        assert!(
            result.is_success(),
            "[{}] Failed to create tab: {}. Exit code: {:?}",
            browser,
            result.stderr,
            result.exit_code()
        );
        println!("Inspect output: {}", result.stdout);
        assert!(
            result.stdout_contains("nav-item") || result.stdout_contains("nav"),
            "[{}] Should find nav content. Got: {}",
            browser,
            result.stdout
        );

        // Small delay to ensure tab URL is updated
        sleep(Duration::from_millis(100)).await;

        // Use empty URL - should stay on same page
        let result = run_webprobe(&[
            "inspect",
            "", // Empty URL!
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "test-tab",
        ]);
        assert!(
            result.is_success(),
            "[{}] Empty URL should work with tabs: {}",
            browser,
            result.stderr
        );
        assert!(
            result.stdout_contains("nav-item") || result.stdout_contains("nav"),
            "[{}] Should still be on same page. Got: {}",
            browser,
            result.stdout
        );

        // Check tab list shows our tab
        let result = run_webprobe(&["tab", "list", "--profile", profile_name]);
        assert!(result.is_success());
        println!("Tab list output: {}", result.stdout);
        assert!(
            result.stdout_contains("test-tab"),
            "[{}] Tab should be listed. Got: {}",
            browser,
            result.stdout
        );

        // Run command without tab - should not break existing tab
        println!("[{}] Running one-shot command...", browser);
        let result = run_webprobe(&["inspect", &test_url, "nav"]);
        assert!(
            result.is_success(),
            "[{}] One-shot command should work: {}",
            browser,
            result.stderr
        );
        println!("[{}] One-shot command completed", browser);

        // Verify tab still works after one-shot command
        let result = run_webprobe(&[
            "inspect",
            "",
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "test-tab",
        ]);
        assert!(
            result.is_success(),
            "[{}] Tab should still be functional: {}",
            browser,
            result.stderr
        );
        // Check that we got some JSON response with the nav element
        assert!(
            result.stdout_contains("\"tag\": \"nav\"")
                || result.stdout_contains("nav-item")
                || result.stdout_contains("nav"),
            "[{}] Tab should still have content. Got: {}",
            browser,
            result.stdout
        );

        // Cleanup daemon after each browser tested
        let _ = run_webprobe(&["daemon", "stop"]);
    }
}

#[tokio::test]
#[serial]
async fn test_authentication_workflow() {
    for browser in get_test_browsers() {
        // Stop and restart daemon for each browser
        ensure_daemon_stopped().await;
        start_daemon_with_browser(browser).await;
        println!("\n=== Testing authentication workflow with {} ===", browser);

        // Check daemon status before each browser
        let status = run_webprobe(&["daemon", "status"]);
        println!("Daemon status before {}: {}", browser, status.stdout);

        let server = ensure_test_server().await;
        let base_url = server.base_url.clone();
        let login_url = format!("{}/login", base_url);

        // Create test profile for authentication workflow
        let profile_name = "auth-profile";
        ensure_test_profile(profile_name, browser);

        // Type email in login form
        let result = run_webprobe(&[
            "type",
            &login_url,
            "#email",
            "test@example.com",
            "--profile",
            profile_name,
            "--tab",
            "auth-session",
        ]);
        assert!(
            result.is_success(),
            "[{}] Failed to type email: {}. Exit code: {:?}",
            browser,
            result.stderr,
            result.exit_code()
        );

        // Type password - empty URL to stay on same page
        let result = run_webprobe(&[
            "type",
            "", // Empty URL - crucial for maintaining session!
            "#password",
            "secret123",
            "--profile",
            profile_name,
            "--tab",
            "auth-session",
        ]);
        assert!(
            result.is_success(),
            "[{}] Failed to type password: {}. Exit code: {:?}",
            browser,
            result.stderr,
            result.exit_code()
        );

        // Click submit button - empty URL again
        let result = run_webprobe(&[
            "click",
            "",
            "button[type=\"submit\"]",
            "--profile",
            profile_name,
            "--tab",
            "auth-session",
        ]);
        assert!(
            result.is_success(),
            "[{}] Failed to click submit: {}. Exit code: {:?}",
            browser,
            result.stderr,
            result.exit_code()
        );

        // Wait for navigation
        sleep(Duration::from_millis(500)).await;

        // Check we're on dashboard (simulated)
        let result = run_webprobe(&[
            "inspect",
            "",
            "h1",
            "--profile",
            profile_name,
            "--tab",
            "auth-session",
        ]);
        assert!(result.is_success(), "[{}] Dashboard check failed", browser);

        // Stop daemon after each browser test
        let _ = run_webprobe(&["daemon", "stop"]);
    }
}

#[tokio::test]
#[serial]
async fn test_mixed_tab_and_oneshot_operations() {
    for browser in get_test_browsers() {
        // Stop and restart daemon for each browser
        ensure_daemon_stopped().await;
        start_daemon_with_browser(browser).await;
        println!("\n=== Testing mixed operations with {} ===", browser);

        let server = ensure_test_server().await;
        let base_url = server.base_url.clone();
        let url1 = format!("{}/elements", base_url);
        let url2 = format!("{}/dashboard", base_url);

        // Create test profile for mixed operations
        let profile_name = "mixed-profile";
        ensure_test_profile(profile_name, browser);

        // Create a persistent tab
        let result = run_webprobe(&[
            "inspect",
            &url1,
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "persistent",
        ]);
        assert!(
            result.is_success(),
            "[{}] Failed to create tab. Exit code: {:?}",
            browser,
            result.exit_code()
        );
        assert!(
            result.stdout_contains("nav-item") || result.stdout_contains("nav"),
            "[{}] Should find nav content. Got: {}",
            browser,
            result.stdout
        );

        // Run one-shot operation on different page
        // Note: Firefox sometimes has issues with multiple WebDriver instances
        if browser == "chrome" {
            let result = run_webprobe(&[
                "inspect", &url2, "h1", // Dashboard has h1, not nav (no --tab flag)
            ]);
            assert!(
                result.is_success(),
                "[{}] One-shot on dashboard failed: {}",
                browser,
                result.stderr
            );
            assert!(
                result.stdout_contains("Dashboard"),
                "[{}] Should find Dashboard",
                browser
            );
        } else {
            println!("[{}] Skipping one-shot operation for Firefox", browser);
        }

        // Verify persistent tab is still on original page
        let result = run_webprobe(&[
            "inspect",
            "", // Empty URL to check current page
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "persistent",
        ]);
        assert!(result.is_success(), "[{}] Tab check failed", browser);
        assert!(
            result.stdout_contains("nav-item") || result.stdout_contains("nav"),
            "[{}] Tab should still be on original page. Got: {}",
            browser,
            result.stdout
        );
        assert!(
            !result.stdout_contains("Dashboard"),
            "[{}] Tab should not have navigated",
            browser
        );

        // Run multiple one-shot operations in parallel
        // Note: Firefox has issues with multiple concurrent WebDriver instances
        if browser == "chrome" {
            let handles: Vec<_> = (0..3)
                .map(|_i| {
                    let url = url2.clone();
                    tokio::spawn(async move {
                        run_webprobe(&[
                            "inspect", &url, "h1", // Dashboard has h1, not nav
                        ])
                    })
                })
                .collect();

            for (i, handle) in handles.into_iter().enumerate() {
                let result = handle.await.unwrap();
                assert!(
                    result.is_success(),
                    "[{}] Parallel operation {} failed: {}",
                    browser,
                    i,
                    result.stderr
                );
            }
        } else {
            println!("[{}] Skipping parallel operations for Firefox", browser);
        }

        // Tab should still be functional
        let result = run_webprobe(&[
            "inspect",
            "",
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "persistent",
        ]);
        assert!(result.is_success(), "[{}] Final tab check failed", browser);
        assert!(
            result.stdout_contains("nav-item") || result.stdout_contains("nav"),
            "[{}] Should find nav content. Got: {}",
            browser,
            result.stdout
        );

        // Stop daemon after each browser test
        let _ = run_webprobe(&["daemon", "stop"]);
    }
}

#[tokio::test]
#[serial]
async fn test_status_command_isolation() {
    for browser in get_test_browsers() {
        // Stop and restart daemon for each browser
        ensure_daemon_stopped().await;
        start_daemon_with_browser(browser).await;
        println!(
            "\n=== Testing status command isolation with {} ===",
            browser
        );

        let server = ensure_test_server().await;
        let base_url = server.base_url.clone();
        let test_url = format!("{}/elements", base_url);

        // Create test profile for status command isolation
        let profile_name = "status-profile";
        ensure_test_profile(profile_name, browser);

        // Create a tab with authentication state (simulated)
        let result = run_webprobe(&[
            "inspect",
            &test_url,
            "body",
            "--profile",
            profile_name,
            "--tab",
            "auth-tab",
        ]);
        assert!(
            result.is_success(),
            "[{}] Failed to create tab. Exit code: {:?}",
            browser,
            result.exit_code()
        );

        // Type something to simulate state change
        let result = run_webprobe(&[
            "eval",
            "document.body.dataset.authenticated = 'true'",
            "--profile",
            profile_name,
            "--tab",
            "auth-tab",
            "--unsafe-eval",
        ]);
        assert!(
            result.is_success(),
            "[{}] Eval command failed: {}",
            browser,
            result.stderr
        );

        // Run status command - this used to kill the session!
        let result = run_webprobe(&["status", "--profile", profile_name, "--tab", "auth-tab"]);
        if !result.is_success() {
            println!("[{}] Status command stderr: {}", browser, result.stderr);
        }
        assert!(
            result.is_success(),
            "[{}] Status command failed: {}",
            browser,
            result.stderr
        );
        println!("[{}] Status output: {}", browser, result.stdout);

        // Verify tab still has authentication state
        let result = run_webprobe(&[
            "eval",
            "document.body.dataset.authenticated",
            "--profile",
            profile_name,
            "--tab",
            "auth-tab",
            "--unsafe-eval",
        ]);
        assert!(
            result.is_success(),
            "[{}] Eval verification failed: {}",
            browser,
            result.stderr
        );
        assert!(
            !result.stdout.is_empty(),
            "[{}] Eval should return authentication state",
            browser
        );

        // Run detect command - another command that shouldn't break state
        let result = run_webprobe(&["detect", "", "--profile", profile_name, "--tab", "auth-tab"]);
        assert!(
            result.is_success(),
            "[{}] Detect command failed. stderr: {}, stdout: {}",
            browser,
            result.stderr,
            result.stdout
        );

        // Verify state is still there
        let result = run_webprobe(&[
            "eval",
            "document.body.dataset.authenticated",
            "--profile",
            profile_name,
            "--tab",
            "auth-tab",
            "--unsafe-eval",
        ]);
        assert!(
            result.is_success(),
            "[{}] Final eval failed: {}",
            browser,
            result.stderr
        );
        assert!(
            !result.stdout.is_empty(),
            "[{}] State should still be accessible",
            browser
        );

        // Stop daemon after each browser test
        let _ = run_webprobe(&["daemon", "stop"]);
    }
}

#[tokio::test]
#[serial]
async fn test_empty_url_handling() {
    for browser in get_test_browsers() {
        // Stop and restart daemon for each browser
        ensure_daemon_stopped().await;
        start_daemon_with_browser(browser).await;
        println!("\n=== Testing empty URL handling with {} ===", browser);

        let server = ensure_test_server().await;
        let base_url = server.base_url.clone();
        let test_url = format!("{}/elements", base_url);

        // Create test profile for empty URL handling
        let profile_name = "empty-profile";
        ensure_test_profile(profile_name, browser);

        // Test 1: Empty URL with existing tab
        let result = run_webprobe(&[
            "inspect",
            &test_url,
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "empty-test",
        ]);
        assert!(result.is_success(), "[{}] Initial inspect failed", browser);

        // Empty URL should not navigate
        let result = run_webprobe(&[
            "inspect",
            "",
            "nav",
            "--profile",
            profile_name,
            "--tab",
            "empty-test",
        ]);
        assert!(
            result.is_success(),
            "[{}] Empty URL failed: {}",
            browser,
            result.stderr
        );
        assert!(
            result.stdout_contains("nav-item") || result.stdout_contains("nav"),
            "[{}] Should find nav content. Got: {}",
            browser,
            result.stdout
        );

        // Test 2: Multiple empty URL commands in sequence
        for i in 0..3 {
            let result = run_webprobe(&[
                "inspect",
                "",
                "nav",
                "--profile",
                profile_name,
                "--tab",
                "empty-test",
            ]);
            assert!(
                result.is_success(),
                "[{}] Empty URL command {} failed",
                browser,
                i
            );
            assert!(
                result.stdout_contains("nav-item") || result.stdout_contains("nav"),
                "[{}] Should find nav content. Got: {}",
                browser,
                result.stdout
            );
        }

        // Test 3: Empty URL with type command
        let result = run_webprobe(&[
            "type",
            "",
            "input[type=\"text\"]",
            "test input",
            "--profile",
            profile_name,
            "--tab",
            "empty-test",
        ]);
        // This might fail if no input exists, but shouldn't crash
        if !result.is_success() {
            assert!(
                result.stderr_contains("not found") || result.stderr_contains("No elements"),
                "[{}] Type command should fail gracefully",
                browser
            );
        }

        // Test 4: Empty URL with click command
        let result = run_webprobe(&[
            "click",
            "",
            "button",
            "--profile",
            profile_name,
            "--tab",
            "empty-test",
        ]);
        // This might fail if no button exists, but shouldn't crash
        if !result.is_success() {
            assert!(
                result.stderr_contains("not found") || result.stderr_contains("No elements"),
                "[{}] Click command should fail gracefully",
                browser
            );
        }

        // Test 5: Empty URL with eval command
        let result = run_webprobe(&[
            "eval",
            "window.location.href",
            "--profile",
            profile_name,
            "--tab",
            "empty-test",
            "--unsafe-eval",
        ]);
        assert!(
            result.is_success(),
            "[{}] Eval should succeed. Error: {}",
            browser,
            result.stderr
        );

        // Test 6: Empty URL without tab (should fail gracefully)
        // Skip for Firefox to avoid WebDriver conflicts
        if browser == "chrome" {
            let result = run_webprobe(&["inspect", "", "nav"]);
            assert!(
                !result.is_success(),
                "[{}] Empty URL without tab should fail",
                browser
            );
        } else {
            println!(
                "[{}] Skipping empty URL without tab test for Firefox",
                browser
            );
        }

        // Stop daemon after each browser test
        let _ = run_webprobe(&["daemon", "stop"]);
    }
}
