// Test utilities for WebDriver tests

use std::sync::Arc;
use tokio::sync::Mutex;
use webprobe::webdriver::{Browser, BrowserType};
use webprobe::webdriver_manager::GLOBAL_WEBDRIVER_MANAGER;

// Global test lock to prevent concurrent WebDriver starts
lazy_static::lazy_static! {
    static ref WEBDRIVER_LOCK: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
}

/// Get a test browser instance with proper WebDriver management
/// This ensures only one test at a time tries to start/use WebDriver
pub async fn get_test_browser_with_retry() -> Option<Browser> {
    // Acquire lock to prevent concurrent WebDriver starts
    let _lock = WEBDRIVER_LOCK.lock().await;

    // Try Chrome first (more reliable for localhost), then Firefox
    for browser_type in &[BrowserType::Chrome, BrowserType::Firefox] {
        // Don't kill existing driver - reuse if available
        // GLOBAL_WEBDRIVER_MANAGER.kill_driver(browser_type);

        // Small delay to ensure cleanup completes
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Try to create browser with retries
        for attempt in 1..=3 {
            match Browser::new(*browser_type, None, None, true).await {
                Ok(browser) => {
                    eprintln!(
                        "Created {} browser on attempt {}",
                        match browser_type {
                            BrowserType::Firefox => "Firefox",
                            BrowserType::Chrome => "Chrome",
                        },
                        attempt
                    );
                    return Some(browser);
                }
                Err(e) => {
                    eprintln!(
                        "Attempt {} failed for {}: {}",
                        attempt,
                        match browser_type {
                            BrowserType::Firefox => "Firefox",
                            BrowserType::Chrome => "Chrome",
                        },
                        e
                    );

                    if attempt < 3 {
                        // Wait before retry
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    eprintln!("WARNING: Could not create test browser after all attempts");
    None
}

/// Clean up WebDriver processes after tests
pub fn cleanup_webdrivers() {
    GLOBAL_WEBDRIVER_MANAGER.stop_all();
}
