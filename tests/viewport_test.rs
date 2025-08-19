/// Integration tests for viewport functionality through the daemon
/// Tests viewport commands and persistence across operations
/// For unit tests of the internal APIs, see simple_viewport_test.rs
use anyhow::Result;
use std::process::Command;

mod common;
use common::{DaemonTestGuard, get_test_browser};

/// Test workflow 1: Create a tab with viewport at local URL and get viewport information
#[tokio::test]
async fn test_viewport_workflow_1() -> Result<()> {
    // Ensure clean state and start daemon
    let mut _guard = DaemonTestGuard::new(get_test_browser());

    // Create a simple HTML page for testing
    let test_html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Viewport Test</title>
            <meta name="viewport" content="width=device-width, initial-scale=1">
        </head>
        <body>
            <h1>Viewport Test Page</h1>
            <div id="viewport-info"></div>
            <script>
                function updateViewport() {
                    const info = document.getElementById('viewport-info');
                    info.textContent = `Width: ${window.innerWidth}, Height: ${window.innerHeight}`;
                }
                updateViewport();
                window.addEventListener('resize', updateViewport);
            </script>
        </body>
        </html>
    "#;

    // Write test HTML to a file
    std::fs::write("/tmp/viewport_test.html", test_html)?;

    // Test 1: Create tab with mobile viewport
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "file:///tmp/viewport_test.html",
            "#viewport-info",
            "--tab",
            "mobile_tab",
            "--viewport",
            "375x812", // iPhone X viewport
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to create tab with viewport: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = String::from_utf8_lossy(&output.stdout);
    println!("Initial viewport response: {}", response);

    // Verify viewport was set (the JavaScript should show the viewport size)
    assert!(
        response.contains("375") || response.contains("Width: 375"),
        "Mobile viewport width not found in response"
    );

    // Test 2: Get viewport information by inspecting the page again
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "", // Empty URL to stay on same page
            "#viewport-info",
            "--tab",
            "mobile_tab",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to get viewport info: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = String::from_utf8_lossy(&output.stdout);
    println!("Viewport info response: {}", response);

    // Verify viewport is still set correctly
    assert!(
        response.contains("375") || response.contains("Width: 375"),
        "Mobile viewport width not maintained"
    );

    // Cleanup happens automatically when guard is dropped
    Ok(())
}

/// Test workflow 2: Go to tab, change viewport, verify change
#[tokio::test]
async fn test_viewport_workflow_2() -> Result<()> {
    // Ensure clean state and start daemon
    let mut _guard = DaemonTestGuard::new(get_test_browser());

    // Create test HTML
    let test_html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Viewport Change Test</title>
            <style>
                #viewport-display {
                    font-size: 24px;
                    padding: 20px;
                    background: #f0f0f0;
                    border-radius: 5px;
                    margin: 20px;
                }
            </style>
        </head>
        <body>
            <h1>Viewport Change Test</h1>
            <div id="viewport-display"></div>
            <script>
                function showViewport() {
                    const display = document.getElementById('viewport-display');
                    display.innerHTML = `
                        <p>Window Width: ${window.innerWidth}px</p>
                        <p>Window Height: ${window.innerHeight}px</p>
                        <p>Device Type: ${window.innerWidth < 768 ? 'Mobile' : 'Desktop'}</p>
                    `;
                }
                showViewport();
                window.addEventListener('resize', showViewport);
            </script>
        </body>
        </html>
    "#;

    std::fs::write("/tmp/viewport_change_test.html", test_html)?;

    // Step 1: Create and go to tab with desktop viewport
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "file:///tmp/viewport_change_test.html",
            "#viewport-display",
            "--tab",
            "resize_tab",
            "--viewport",
            "1920x1080", // Desktop viewport
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to create tab: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = String::from_utf8_lossy(&output.stdout);
    println!("Initial desktop viewport: {}", response);
    assert!(
        response.contains("1920") || response.contains("Desktop"),
        "Desktop viewport not set correctly"
    );

    // Step 2: Change viewport to mobile
    // Note: We need to use a command that allows viewport change
    // For now, we'll navigate to the same URL with a different viewport
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "file:///tmp/viewport_change_test.html",
            "#viewport-display",
            "--tab",
            "resize_tab",
            "--viewport",
            "375x667", // iPhone 6/7/8 viewport
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to change viewport: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = String::from_utf8_lossy(&output.stdout);
    println!("Changed to mobile viewport: {}", response);

    // Step 3: Verify viewport changed
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "eval",
            "", // Stay on same page
            "JSON.stringify({width: window.innerWidth, height: window.innerHeight})",
            "--tab",
            "resize_tab",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to verify viewport: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response = String::from_utf8_lossy(&output.stdout);
    println!("Viewport verification: {}", response);

    // Parse JSON response to verify viewport
    if response.contains("width") {
        assert!(
            response.contains("375"),
            "Mobile viewport width not found after change"
        );
    }

    // Cleanup happens automatically when guard is dropped
    Ok(())
}

/// Test workflow 3: Multiple tabs with different viewports
#[tokio::test]
async fn test_viewport_multiple_tabs() -> Result<()> {
    // Ensure clean state and start daemon
    let mut _guard = DaemonTestGuard::new(get_test_browser());

    // Create responsive test HTML
    let test_html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Responsive Test</title>
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <style>
                body { font-family: Arial, sans-serif; margin: 0; padding: 20px; }
                .container {
                    max-width: 1200px;
                    margin: 0 auto;
                    padding: 20px;
                    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                    color: white;
                    border-radius: 10px;
                }
                .viewport-info {
                    font-size: 18px;
                    padding: 15px;
                    background: rgba(255,255,255,0.2);
                    border-radius: 5px;
                    margin: 10px 0;
                }
                @media (max-width: 768px) {
                    .container { background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%); }
                    .device-type::after { content: "Mobile View"; }
                }
                @media (min-width: 769px) {
                    .device-type::after { content: "Desktop View"; }
                }
            </style>
        </head>
        <body>
            <div class="container">
                <h1>Responsive Design Test</h1>
                <div class="viewport-info">
                    <div class="device-type">Device: </div>
                    <div id="dimensions"></div>
                </div>
            </div>
            <script>
                document.getElementById('dimensions').textContent = 
                    `${window.innerWidth} x ${window.innerHeight}`;
            </script>
        </body>
        </html>
    "#;

    std::fs::write("/tmp/responsive_test.html", test_html)?;

    // Create mobile tab
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "file:///tmp/responsive_test.html",
            ".device-type",
            "--tab",
            "mobile",
            "--viewport",
            "414x896", // iPhone 11 Pro Max
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to create mobile tab: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mobile_response = String::from_utf8_lossy(&output.stdout);
    println!("Mobile tab response: {}", mobile_response);
    assert!(
        mobile_response.contains("Mobile"),
        "Mobile view not detected"
    );

    // Create desktop tab
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "file:///tmp/responsive_test.html",
            ".device-type",
            "--tab",
            "desktop",
            "--viewport",
            "1920x1080",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to create desktop tab: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let desktop_response = String::from_utf8_lossy(&output.stdout);
    println!("Desktop tab response: {}", desktop_response);
    assert!(
        desktop_response.contains("Desktop"),
        "Desktop view not detected"
    );

    // Verify mobile tab still has mobile viewport
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "", // Stay on same page
            "#dimensions",
            "--tab",
            "mobile",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to verify mobile tab: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mobile_check = String::from_utf8_lossy(&output.stdout);
    println!("Mobile tab verification: {}", mobile_check);
    assert!(
        mobile_check.contains("414"),
        "Mobile viewport not maintained"
    );

    // Verify desktop tab still has desktop viewport
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "inspect",
            "", // Stay on same page
            "#dimensions",
            "--tab",
            "desktop",
        ])
        .output()?;

    assert!(
        output.status.success(),
        "Failed to verify desktop tab: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let desktop_check = String::from_utf8_lossy(&output.stdout);
    println!("Desktop tab verification: {}", desktop_check);
    assert!(
        desktop_check.contains("1920"),
        "Desktop viewport not maintained"
    );

    // Cleanup happens automatically when guard is dropped
    Ok(())
}
