use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::types::ViewportSize;
use crate::webdriver::{Browser, BrowserType};

/// A pool of browser instances for reuse
pub struct BrowserPool {
    /// Available browsers ready for use
    available: Arc<Mutex<VecDeque<PooledBrowser>>>,
    /// Maximum number of browsers to keep in pool
    max_size: usize,
}

struct PooledBrowser {
    browser: Browser,
    browser_type: BrowserType,
    headless: bool,
    created_at: std::time::Instant,
}

impl BrowserPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            available: Arc::new(Mutex::new(VecDeque::new())),
            max_size,
        }
    }

    /// Get a browser from the pool or create a new one
    pub async fn get(
        &self,
        browser_type: BrowserType,
        viewport: Option<ViewportSize>,
        headless: bool,
    ) -> Result<Browser> {
        let mut pool = self.available.lock().await;

        // Try to find a matching browser in the pool
        let mut found_index = None;
        for (i, pooled) in pool.iter().enumerate() {
            if pooled.browser_type == browser_type && pooled.headless == headless {
                // Check if browser is still healthy (not too old)
                if pooled.created_at.elapsed() < std::time::Duration::from_secs(300) {
                    found_index = Some(i);
                    break;
                }
            }
        }

        if let Some(index) = found_index {
            // Found a matching browser, remove it from pool
            let pooled = pool.remove(index).unwrap();
            info!("Reusing browser from pool (pool size: {})", pool.len());

            // Test if the browser is still alive by getting current URL
            match pooled.browser.get_current_url().await {
                Ok(_) => {
                    // Browser is healthy, clear any state by navigating to about:blank
                    // This ensures we don't have stale element references
                    if let Err(e) = pooled.browser.goto("about:blank").await {
                        info!("Failed to clear browser state ({}), creating new one", e);
                        Browser::new(browser_type, None, viewport, headless).await
                    } else {
                        // Give the browser a moment to clear state
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        Ok(pooled.browser)
                    }
                }
                Err(e) => {
                    // Browser connection is dead, create a new one
                    info!("Pooled browser is dead ({}), creating new one", e);
                    Browser::new(browser_type, None, viewport, headless).await
                }
            }
        } else {
            // No matching browser, create a new one
            info!("Creating new browser (no match in pool of {})", pool.len());
            Browser::new(browser_type, None, viewport, headless).await
        }
    }

    /// Return a browser to the pool for reuse
    pub async fn return_browser(
        &self,
        browser: Browser,
        browser_type: BrowserType,
        headless: bool,
    ) -> Result<()> {
        let mut pool = self.available.lock().await;

        // Only keep browser if pool isn't full
        if pool.len() < self.max_size {
            // Navigate to about:blank to minimize memory usage
            browser.goto("about:blank").await?;

            pool.push_back(PooledBrowser {
                browser,
                browser_type,
                headless,
                created_at: std::time::Instant::now(),
            });

            debug!("Returned browser to pool (pool size: {})", pool.len());
        } else {
            // Pool is full, close the browser
            debug!("Pool is full, closing browser");
            browser.close().await?;
        }

        Ok(())
    }

    /// Clean up old or dead browsers in the pool
    pub async fn cleanup(&self) -> Result<()> {
        let mut pool = self.available.lock().await;
        let mut to_remove = Vec::new();

        // Find browsers that are old or dead
        for (i, pooled) in pool.iter().enumerate() {
            // Remove if older than 5 minutes
            if pooled.created_at.elapsed() > std::time::Duration::from_secs(300) {
                to_remove.push(i);
            } else {
                // Check if browser is still alive by sending a simple command
                if pooled.browser.get_current_url().await.is_err() {
                    debug!("Found dead browser in pool at index {}", i);
                    to_remove.push(i);
                }
            }
        }

        // Remove old/dead browsers (in reverse order to maintain indices)
        for i in to_remove.iter().rev() {
            if let Some(pooled) = pool.remove(*i) {
                let _ = pooled.browser.close().await;
            }
        }

        if !to_remove.is_empty() {
            info!("Cleaned up {} old/dead browsers from pool", to_remove.len());
        }

        Ok(())
    }

    /// Close all browsers in the pool
    pub async fn close_all(&self) -> Result<()> {
        let mut pool = self.available.lock().await;

        while let Some(pooled) = pool.pop_front() {
            let _ = pooled.browser.close().await;
        }

        info!("Closed all browsers in pool");
        Ok(())
    }
}

// Global browser pool for the daemon
lazy_static::lazy_static! {
    pub static ref GLOBAL_BROWSER_POOL: BrowserPool = BrowserPool::new(3);
}
