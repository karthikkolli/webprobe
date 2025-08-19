use anyhow::Result;
use std::process::Command;
use tokio::runtime::Runtime;

mod test_server;

/// Helper to run webprobe command
fn run_webprobe(args: &[&str]) -> std::process::Output {
    Command::new("./target/debug/webprobe")
        .args(args)
        .output()
        .expect("Failed to run webprobe")
}

/// Test that commands work on the default "main" tab when no --tab is specified with a profile
#[test]
fn test_default_tab_workflow() -> Result<()> {
    // Start test server
    let rt = Runtime::new()?;
    let server = rt.block_on(async { test_server::ensure_test_server().await });
    let test_url = format!("{}/test", server.base_url);
    // Start daemon
    let _start_result = run_webprobe(&["daemon", "start", "--browser", "chrome"]);

    // Give daemon time to start
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Verify daemon is running
    let status = run_webprobe(&["daemon", "status"]);
    assert!(
        String::from_utf8_lossy(&status.stdout).contains("Daemon is running"),
        "Daemon should be running"
    );

    // Clean up any existing profile first
    let _ = run_webprobe(&["profile", "destroy", "test-default-tab", "--force"]);

    // Create a test profile
    let result = run_webprobe(&[
        "profile",
        "create",
        "test-default-tab",
        "--browser",
        "chrome",
    ]);
    if !result.status.success() {
        eprintln!(
            "Profile create failed. stderr: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        eprintln!("stdout: {}", String::from_utf8_lossy(&result.stdout));
    }
    assert!(result.status.success(), "Should create test profile");

    println!("Testing default tab workflow with URL: {}", test_url);

    // Test 1: Inspect with profile but no --tab should use default "main" tab
    let result = run_webprobe(&["inspect", &test_url, "h1", "--profile", "test-default-tab"]);
    assert!(
        result.status.success(),
        "Inspect with profile but no --tab should succeed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        output.contains("Test Page") || output.contains("\"text\":\"Test Page\""),
        "Should find h1 content: {}",
        output
    );

    // Test 2: Inspect again with profile but no --tab should reuse the same default tab
    let result = run_webprobe(&[
        "inspect",
        "", // Empty URL to stay on same page
        "body",
        "--profile",
        "test-default-tab",
    ]);
    assert!(
        result.status.success(),
        "Second inspect with profile but no --tab should succeed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Test 6: Scroll with profile but no --tab uses default tab
    let result = run_webprobe(&[
        "scroll",
        "", // Empty URL to stay on same page
        "--by-y",
        "100",
        "--profile",
        "test-default-tab",
    ]);
    assert!(
        result.status.success(),
        "Scroll with profile but no --tab should succeed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Test 7: Analyze with profile but no --tab uses default tab
    let result = run_webprobe(&[
        "analyze",
        "", // Empty URL to stay on same page
        "body",
        "--focus",
        "spacing",
        "--profile",
        "test-default-tab",
    ]);
    assert!(
        result.status.success(),
        "Analyze with profile but no --tab should succeed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Test 8: Layout with profile but no --tab uses default tab
    let result = run_webprobe(&[
        "layout",
        "", // Empty URL to stay on same page
        "body",
        "--depth",
        "2",
        "--profile",
        "test-default-tab",
    ]);
    assert!(
        result.status.success(),
        "Layout with profile but no --tab should succeed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Test 9: Commands without profile should be one-shot operations (require URL)
    let result = run_webprobe(&["inspect", "", "h1"]); // Empty URL without profile
    assert!(
        !result.status.success(),
        "Inspect without profile and empty URL should fail"
    );
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("URL is required"),
        "Should error about URL required for one-shot: {}",
        error
    );

    // Test 10: --tab without --profile should error
    let result = run_webprobe(&["inspect", &test_url, "h1", "--tab", "sometab"]);
    assert!(
        !result.status.success(),
        "--tab without --profile should fail"
    );
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("--tab requires --profile"),
        "Should error about --tab requiring --profile: {}",
        error
    );

    // Clean up
    let _ = run_webprobe(&["profile", "destroy", "test-default-tab"]);
    let _ = run_webprobe(&["daemon", "stop"]);

    Ok(())
}

/// Test that one-shot operations (no profile) and profile operations don't interfere
#[test]
fn test_oneshot_vs_default_tab() -> Result<()> {
    // Start test server
    let rt = Runtime::new()?;
    let server = rt.block_on(async { test_server::ensure_test_server().await });
    // Start daemon
    let _ = run_webprobe(&["daemon", "start", "--browser", "chrome"]);
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Clean up any existing profile first
    let _ = run_webprobe(&["profile", "destroy", "test-oneshot", "--force"]);

    // Create a test profile
    let result = run_webprobe(&["profile", "create", "test-oneshot", "--browser", "chrome"]);
    assert!(result.status.success(), "Should create test profile");

    // Use different URLs
    let url1 = format!("{}/test", server.base_url);
    let url2 = format!("{}/elements", server.base_url);

    // Navigate profile's main tab to url1
    let result = run_webprobe(&["inspect", &url1, "h1", "--profile", "test-oneshot"]);
    assert!(
        result.status.success(),
        "Should navigate profile's main tab to url1"
    );

    // Do a one-shot operation on url2 (should not affect profile's main tab)
    println!("Running one-shot operation on url2: {}", url2);
    let result = run_webprobe(&["inspect", &url2, ".nav-item"]);
    if !result.status.success() {
        eprintln!(
            "One-shot operation failed. stderr: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        eprintln!("stdout: {}", String::from_utf8_lossy(&result.stdout));
    }
    assert!(result.status.success(), "One-shot operation should succeed");
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        output.contains("Home") || output.contains("\"text\":\"Home\""),
        "One-shot should see elements page nav: {}",
        output
    );

    // Profile's main tab should still be on url1
    let result = run_webprobe(&["inspect", "", "h1", "--profile", "test-oneshot"]);
    assert!(
        result.status.success(),
        "Profile's main tab should still work"
    );
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        output.contains("Test Page") || output.contains("\"text\":\"Test Page\""),
        "Profile's main tab should still be on test page: {}",
        output
    );

    // Clean up
    let _ = run_webprobe(&["profile", "destroy", "test-oneshot"]);
    let _ = run_webprobe(&["daemon", "stop"]);

    Ok(())
}

/// Test profile isolation with default tabs
#[test]
fn test_profile_default_tabs() -> Result<()> {
    // Start test server
    let rt = Runtime::new()?;
    let server = rt.block_on(async { test_server::ensure_test_server().await });
    // Start daemon
    let _ = run_webprobe(&["daemon", "start", "--browser", "chrome"]);
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Clean up any existing profiles first
    let _ = run_webprobe(&["profile", "destroy", "test-profile-1", "--force"]);
    let _ = run_webprobe(&["profile", "destroy", "test-profile-2", "--force"]);

    // Create two profiles
    let result = run_webprobe(&["profile", "create", "test-profile-1", "--browser", "chrome"]);
    assert!(
        result.status.success(),
        "Should create profile 1: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let result = run_webprobe(&["profile", "create", "test-profile-2", "--browser", "chrome"]);
    assert!(
        result.status.success(),
        "Should create profile 2: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let url1 = format!("{}/test", server.base_url);
    let url2 = format!("{}/elements", server.base_url);

    // Navigate profile 1's default tab to url1
    let result = run_webprobe(&[
        "inspect",
        &url1,
        "h1",
        "--profile",
        "test-profile-1",
        "--tab",
        "main",
    ]);
    assert!(
        result.status.success(),
        "Should navigate profile 1 to url1: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Navigate profile 2's default tab to url2
    let result = run_webprobe(&[
        "inspect",
        &url2,
        ".nav-item",
        "--profile",
        "test-profile-2",
        "--tab",
        "main",
    ]);
    assert!(
        result.status.success(),
        "Should navigate profile 2 to url2: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Check profile 1 is still on url1
    let result = run_webprobe(&[
        "inspect",
        "",
        "h1",
        "--profile",
        "test-profile-1",
        "--tab",
        "main",
    ]);
    assert!(
        result.status.success(),
        "Profile 1 should still work: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        output.contains("Test Page") || output.contains("\"text\":\"Test Page\""),
        "Profile 1 should be on test page: {}",
        output
    );

    // Check profile 2 is still on url2
    let result = run_webprobe(&[
        "inspect",
        "",
        ".nav-item",
        "--profile",
        "test-profile-2",
        "--tab",
        "main",
    ]);
    assert!(
        result.status.success(),
        "Profile 2 should still work: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8_lossy(&result.stdout);
    assert!(
        output.contains("Home") || output.contains("\"text\":\"Home\""),
        "Profile 2 should be on elements page with nav: {}",
        output
    );

    // Clean up profiles
    let _ = run_webprobe(&["profile", "destroy", "test-profile-1"]);
    let _ = run_webprobe(&["profile", "destroy", "test-profile-2"]);
    let _ = run_webprobe(&["daemon", "stop"]);

    Ok(())
}
