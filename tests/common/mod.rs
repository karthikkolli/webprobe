// Common test utilities and fixtures

use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary directory for test profiles
#[allow(dead_code)]
pub fn create_temp_profile_dir() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let path = temp_dir.path().to_path_buf();
    (temp_dir, path)
}

/// Mock HTML pages for testing
pub mod fixtures {
    pub const SIMPLE_PAGE: &str = r#"
    <!DOCTYPE html>
    <html>
    <head><title>Test Page</title></head>
    <body>
        <h1>Test Header</h1>
        <div id="content">Test content</div>
        <button class="btn">Click me</button>
    </body>
    </html>
    "#;

    pub const PAGE_WITH_CONSOLE: &str = r#"
    <!DOCTYPE html>
    <html>
    <head><title>Console Test</title></head>
    <body>
        <div id="app">App</div>
        <script>
            // Delay logs slightly to ensure our capture is set up
            setTimeout(() => {
                console.log("Page loaded");
                console.error("Test error");
                console.warn("Test warning");
                console.info("Test info");
            }, 50);
            
            // Also test immediate logs (may not be captured)
            console.log("Immediate log");
        </script>
    </body>
    </html>
    "#;

    #[allow(dead_code)]
    pub const BROKEN_PAGE: &str = r#"
    <!DOCTYPE html>
    <html>
    <head><title>Broken Page</title></head>
    <body>
        <div id="error">Error</div>
        <script>
            throw new Error("Page initialization failed");
        </script>
    </body>
    </html>
    "#;
}

/// Helper to create a test HTML file
pub fn create_test_html(content: &str) -> PathBuf {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("test.html");
    std::fs::write(&file_path, content).expect("Failed to write test HTML");

    // Leak the temp_dir to keep it alive for the test
    std::mem::forget(temp_dir);
    file_path
}
