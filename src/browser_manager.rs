use anyhow::{Context, Result};
use dashmap::DashMap;
use fantoccini::wd::WindowHandle;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};
use tracing::{debug, error, info, warn};

use crate::types::{ElementInfo, InspectionDepth, ViewportSize};
use crate::webdriver::{Browser, BrowserType};

/// State of a browser tab
#[derive(Debug, Clone)]
pub enum TabState {
    /// Tab is healthy and operational
    Healthy,
    /// Tab has encountered an error and is non-functional
    Broken(String),
    /// Tab is in the process of being closed
    Closing,
}

/// Context for safe tab operations that prevents cross-tab interference
pub struct TabContext<'a> {
    /// Reference to the browser
    browser: &'a Browser,
    /// Name of the current tab
    tab_name: String,
    /// Lock guard that ensures exclusive access to this tab
    _guard: MutexGuard<'a, ()>,
}

impl<'a> TabContext<'a> {
    /// Click an element (safe operation that can't switch tabs)
    pub async fn click_element(&self, selector: &str, index: Option<usize>) -> Result<()> {
        self.browser.click_element("", selector, index).await
    }

    /// Type text into an element
    pub async fn type_text(&self, selector: &str, text: &str, clear: bool) -> Result<()> {
        self.browser.type_text("", selector, text, clear).await
    }

    /// Inspect an element
    pub async fn inspect_element(
        &self,
        selector: &str,
        depth: InspectionDepth,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
    ) -> Result<Vec<ElementInfo>> {
        self.browser
            .inspect_element("", selector, depth, all, index, expect_one)
            .await
    }

    /// Navigate to a URL
    pub async fn goto(&self, url: &str) -> Result<()> {
        self.browser.goto(url).await
    }

    /// Execute JavaScript
    pub async fn execute_javascript(&self, code: &str) -> Result<serde_json::Value> {
        self.browser.execute_javascript(Some(""), code).await
    }

    /// Execute JavaScript (alias for compatibility)
    pub async fn execute_script(&self, code: &str) -> Result<serde_json::Value> {
        self.browser.execute_javascript(Some(""), code).await
    }

    /// Detect smart elements on the page
    pub async fn detect_smart_elements(&self, context: Option<&str>) -> Result<serde_json::Value> {
        self.browser.detect_smart_elements("", context).await
    }

    /// Diagnose layout issues
    pub async fn diagnose_layout(
        &self,
        selector: Option<&str>,
        check_type: &str,
    ) -> Result<serde_json::Value> {
        self.browser.diagnose_layout(selector, check_type).await
    }

    /// Scroll the page
    pub async fn scroll(
        &self,
        selector: Option<&str>,
        by_x: i32,
        by_y: i32,
        to: Option<&str>,
    ) -> Result<()> {
        self.browser.scroll("", selector, by_x, by_y, to).await
    }

    /// Get the tab name
    pub fn tab_name(&self) -> &str {
        &self.tab_name
    }

    /// Set viewport size for this tab using JavaScript emulation
    /// This is a workaround since we can't use CDP directly through fantoccini
    pub async fn set_viewport(&self, viewport: &ViewportSize) -> Result<()> {
        // Override window.innerWidth and window.innerHeight
        let script = format!(
            r#"
            // Store original viewport for restoration if needed
            window.__originalViewport = window.__originalViewport || {{
                width: window.innerWidth,
                height: window.innerHeight
            }};
            
            // Override viewport dimensions
            Object.defineProperty(window, 'innerWidth', {{
                writable: true,
                configurable: true,
                value: {}
            }});
            Object.defineProperty(window, 'innerHeight', {{
                writable: true,
                configurable: true,
                value: {}
            }});
            
            // Trigger resize event for responsive layouts
            window.dispatchEvent(new Event('resize'));
            
            return {{ width: window.innerWidth, height: window.innerHeight }};
            "#,
            viewport.width, viewport.height
        );

        self.browser.execute_javascript(Some(""), &script).await?;
        Ok(())
    }

    /// Validate the page for accessibility, SEO, and performance issues
    pub async fn validate_page(&self, check_type: &str) -> Result<serde_json::Value> {
        self.browser.validate_page(check_type).await
    }

    /// Compare two pages
    pub async fn compare_pages(
        &self,
        url1: &str,
        url2: &str,
        mode: &str,
        selector: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.browser.compare_pages(url1, url2, mode, selector).await
    }
}

/// Manages a single browser instance with multiple tabs/windows
#[derive(Debug)]
pub struct BrowserManager {
    /// The actual browser connection
    browser: Browser,
    /// Mapping of tab names to window handles
    tabs: Arc<Mutex<HashMap<String, WindowHandle>>>,
    /// The currently active tab name
    current_tab: Arc<Mutex<Option<String>>>,
    /// Browser type for this manager
    browser_type: BrowserType,
    /// Set of temporary tabs that should be cleaned up after use
    temporary_tabs: Arc<Mutex<HashSet<String>>>,
    /// Per-tab locks for operation serialization (lock-free concurrent access)
    tab_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
    /// Tab health states for fail-fast behavior
    tab_states: Arc<DashMap<String, TabState>>,
    /// Mutex to serialize window creation (WebDriver limitation)
    window_creation_lock: Arc<Mutex<()>>,
    /// Per-tab viewport settings (using JavaScript emulation)
    tab_viewports: Arc<DashMap<String, ViewportSize>>,
}

impl BrowserManager {
    /// Create a new browser manager with a single browser instance
    pub async fn new(
        browser_type: BrowserType,
        profile: Option<String>,
        viewport: Option<ViewportSize>,
        headless: bool,
    ) -> Result<Self> {
        info!("Creating BrowserManager with {:?}", browser_type);

        // Create the browser instance
        let browser = Browser::new(browser_type, profile, viewport, headless).await?;

        // Get the initial window handle
        let initial_handle = browser.client.window().await?;

        let mut tabs = HashMap::new();
        tabs.insert("main".to_string(), initial_handle);

        // Initialize per-tab lock and state for main tab
        let tab_locks = Arc::new(DashMap::new());
        tab_locks.insert("main".to_string(), Arc::new(Mutex::new(())));

        let tab_states = Arc::new(DashMap::new());
        tab_states.insert("main".to_string(), TabState::Healthy);

        Ok(Self {
            browser,
            tabs: Arc::new(Mutex::new(tabs)),
            current_tab: Arc::new(Mutex::new(Some("main".to_string()))),
            browser_type,
            temporary_tabs: Arc::new(Mutex::new(HashSet::new())),
            tab_locks,
            tab_states,
            window_creation_lock: Arc::new(Mutex::new(())),
            tab_viewports: Arc::new(DashMap::new()),
        })
    }

    /// Create a new tab and return its name
    pub async fn create_tab(&self, name: String) -> Result<()> {
        info!("Creating new tab: {}", name);

        // Check if tab is already being closed
        if let Some(state) = self.tab_states.get(&name)
            && matches!(state.value(), TabState::Closing)
        {
            return Err(anyhow::anyhow!("Tab '{}' is being closed", name));
        }

        // Serialize window creation to avoid WebDriver race conditions
        let _creation_guard = self.window_creation_lock.lock().await;
        debug!("Acquired window creation lock for tab '{}'", name);

        // Ensure we're on a valid window before creating a new one
        // WebDriver requires an active window context to create new windows/tabs
        let current_windows = self
            .browser
            .client
            .windows()
            .await
            .context("Failed to get current windows")?;
        if !current_windows.is_empty() {
            // Switch to the first available window to ensure we have a valid context
            self.browser
                .client
                .switch_to_window(current_windows[0].clone())
                .await
                .context("Failed to switch to existing window")?;
            debug!("Switched to existing window before creating new tab");
        }

        // Create a new tab (true = tab, false = window)
        let new_handle = match self.browser.client.new_window(true).await {
            Ok(handle) => {
                debug!("Successfully created new tab for '{}'", name);
                handle
            }
            Err(e) => {
                error!("Failed to create new tab for '{}': {:?}", name, e);
                return Err(anyhow::anyhow!("Failed to create new tab: {}", e));
            }
        };

        // Store the mapping
        let mut tabs = self.tabs.lock().await;
        if tabs.contains_key(&name) {
            anyhow::bail!("Tab '{}' already exists", name);
        }
        tabs.insert(name.clone(), new_handle.handle);

        // Create per-tab lock and mark as healthy
        self.tab_locks
            .insert(name.clone(), Arc::new(Mutex::new(())));
        self.tab_states.insert(name.clone(), TabState::Healthy);

        debug!("Created tab '{}' with handle", name);
        Ok(())
    }

    /// Switch to a specific tab by name (internal use - assumes caller holds tab lock)
    async fn switch_to_tab_internal(&self, name: &str) -> Result<()> {
        debug!("Switching to tab: {}", name);

        // Check tab health first
        if let Some(state) = self.tab_states.get(name) {
            match state.value() {
                TabState::Broken(err) => {
                    return Err(anyhow::anyhow!("Tab '{}' is broken: {}", name, err));
                }
                TabState::Closing => {
                    return Err(anyhow::anyhow!("Tab '{}' is being closed", name));
                }
                TabState::Healthy => {}
            }
        }

        let tabs = self.tabs.lock().await;
        let handle = tabs
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Tab '{}' not found", name))?;

        // Try to switch to the window
        match self.browser.client.switch_to_window(handle.clone()).await {
            Ok(_) => {
                // Update current tab
                let mut current = self.current_tab.lock().await;
                *current = Some(name.to_string());
                debug!("Switched to tab '{}'", name);

                // Restore viewport for this tab if we have one stored
                if let Some(viewport) = self.tab_viewports.get(name) {
                    debug!(
                        "Restoring viewport {}x{} for tab '{}'",
                        viewport.width, viewport.height, name
                    );
                    // Use JavaScript to emulate the viewport
                    let script = format!(
                        r#"
                        Object.defineProperty(window, 'innerWidth', {{
                            writable: true,
                            configurable: true,
                            value: {}
                        }});
                        Object.defineProperty(window, 'innerHeight', {{
                            writable: true,
                            configurable: true,
                            value: {}
                        }});
                        window.dispatchEvent(new Event('resize'));
                        "#,
                        viewport.width, viewport.height
                    );
                    if let Err(e) = self.browser.execute_javascript(Some(""), &script).await {
                        warn!("Failed to restore viewport for tab '{}': {}", name, e);
                    }
                }

                Ok(())
            }
            Err(e) => {
                // Mark tab as broken
                self.tab_states
                    .insert(name.to_string(), TabState::Broken(e.to_string()));
                Err(anyhow::anyhow!("Failed to switch to tab '{}': {}", name, e))
            }
        }
    }

    /// Public method to switch tabs (acquires lock first)
    pub async fn switch_to_tab(&self, name: &str) -> Result<()> {
        // Get the tab's lock
        let tab_lock = self
            .tab_locks
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Tab '{}' not found", name))?
            .clone();

        let _guard = tab_lock.lock().await;
        self.switch_to_tab_internal(name).await
    }

    /// Close a tab by name (race-free implementation)
    pub async fn close_tab(&self, name: &str) -> Result<()> {
        info!("Closing tab: {}", name);

        // Step 1: Mark as closing (prevents new operations)
        self.tab_states.insert(name.to_string(), TabState::Closing);

        // Step 2: Get the lock and wait for current operations
        let tab_lock = self.tab_locks.get(name).map(|e| e.clone());

        if let Some(lock) = tab_lock {
            // Acquire lock to wait for any ongoing operations
            let _guard = lock.lock().await;

            // Step 3: Remove tab handle FIRST (while holding lock)
            let mut tabs = self.tabs.lock().await;

            // Don't allow closing the main tab if it's the only one
            if tabs.len() == 1 && name == "main" {
                // Restore healthy state and bail
                self.tab_states.insert(name.to_string(), TabState::Healthy);
                anyhow::bail!("Cannot close the last tab");
            }

            let handle = tabs
                .remove(name)
                .ok_or_else(|| anyhow::anyhow!("Tab '{}' not found", name))?;

            // If we closed the current tab, update current to another tab
            let mut current = self.current_tab.lock().await;
            if current.as_ref() == Some(&name.to_string()) {
                // Switch to the first available tab
                if let Some((first_name, first_handle)) = tabs.iter().next() {
                    self.browser
                        .client
                        .switch_to_window(first_handle.clone())
                        .await?;
                    *current = Some(first_name.clone());
                } else {
                    *current = None;
                }
            }
            drop(tabs);
            drop(current);

            // Step 4: Close the actual window
            // First switch to it if we can
            if let Err(e) = self.browser.client.switch_to_window(handle.clone()).await {
                warn!("Failed to switch to tab '{}' before closing: {}", name, e);
            }

            if let Err(e) = self.browser.client.close_window().await {
                warn!("Failed to close window for tab '{}': {}", name, e);
            }
        }

        // Step 5: Clean up tracking structures LAST
        self.tab_locks.remove(name);
        self.tab_states.remove(name);

        let mut temp_tabs = self.temporary_tabs.lock().await;
        temp_tabs.remove(name);

        debug!("Closed tab '{}'", name);
        Ok(())
    }

    /// Get or create a tab with the given name (does NOT switch to it to avoid deadlock with with_tab)
    pub async fn get_or_create_tab(&self, name: &str) -> Result<()> {
        info!("get_or_create_tab called for '{}'", name);
        let tabs = self.tabs.lock().await;
        if !tabs.contains_key(name) {
            info!("Tab '{}' doesn't exist, creating it", name);
            drop(tabs); // Release lock before creating
            self.create_tab(name.to_string()).await?;
        } else {
            info!("Tab '{}' already exists", name);
        }
        // Don't switch here - with_tab will do it to avoid deadlock
        Ok(())
    }

    /// List all open tabs
    pub async fn list_tabs(&self) -> Vec<String> {
        let tabs = self.tabs.lock().await;
        tabs.keys().cloned().collect()
    }

    /// Get the current tab name
    pub async fn current_tab_name(&self) -> Option<String> {
        let current = self.current_tab.lock().await;
        current.clone()
    }

    /// Get the browser instance for operations
    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    /// Navigate in the current tab
    pub async fn goto(&self, url: &str) -> Result<()> {
        self.browser.goto(url).await
    }

    /// Set viewport for a specific tab
    pub async fn set_tab_viewport(&self, tab_name: &str, viewport: ViewportSize) -> Result<()> {
        // Store the viewport for this tab
        self.tab_viewports.insert(tab_name.to_string(), viewport);

        // If this is the current tab, apply it immediately
        let current = self.current_tab.lock().await;
        if current.as_ref().map(|s| s.as_str()) == Some(tab_name) {
            let script = format!(
                r#"
                Object.defineProperty(window, 'innerWidth', {{
                    writable: true,
                    configurable: true,
                    value: {}
                }});
                Object.defineProperty(window, 'innerHeight', {{
                    writable: true,
                    configurable: true,
                    value: {}
                }});
                window.dispatchEvent(new Event('resize'));
                "#,
                viewport.width, viewport.height
            );
            self.browser.execute_javascript(Some(""), &script).await?;
        }

        Ok(())
    }

    /// Get viewport for a specific tab
    pub fn get_tab_viewport(&self, tab_name: &str) -> Option<ViewportSize> {
        self.tab_viewports.get(tab_name).map(|v| *v)
    }

    /// Execute an operation in a specific tab (with per-tab locking for parallelism)
    pub async fn with_tab<F, R>(&self, tab_name: &str, operation: F) -> Result<R>
    where
        F: for<'a> FnOnce(
            TabContext<'a>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<R>> + Send + 'a>,
        >,
        R: Send + 'static,
    {
        info!("with_tab called for tab '{}'", tab_name);

        // Check tab health (fail-fast on broken tabs)
        if let Some(state) = self.tab_states.get(tab_name) {
            match state.value() {
                TabState::Broken(err) => {
                    return Err(anyhow::anyhow!("Tab '{}' is broken: {}", tab_name, err));
                }
                TabState::Closing => {
                    return Err(anyhow::anyhow!("Tab '{}' is being closed", tab_name));
                }
                TabState::Healthy => {}
            }
        } else {
            warn!("Tab '{}' not found in tab_states", tab_name);
        }

        // Get the tab's lock (without holding the locks map)
        let tab_lock = self
            .tab_locks
            .get(tab_name)
            .ok_or_else(|| {
                error!("Tab '{}' not found in tab_locks", tab_name);
                anyhow::anyhow!("Tab '{}' not found", tab_name)
            })?
            .clone();

        // Lock this specific tab (other tabs can still operate)
        let guard = tab_lock.lock().await;
        debug!("Acquired lock for tab '{}'", tab_name);

        // Switch to the tab (while holding the tab lock)
        self.switch_to_tab_internal(tab_name).await?;

        // Create safe context
        let context = TabContext {
            browser: &self.browser,
            tab_name: tab_name.to_string(),
            _guard: guard,
        };

        // Execute the operation with safe context
        let result = operation(context).await;

        debug!("Released lock for tab '{}'", tab_name);
        result
    }

    /// Execute an operation in a temporary tab (for one-shot operations)
    pub async fn with_temp_tab<F, R>(&self, operation: F) -> Result<R>
    where
        F: for<'a> FnOnce(
            TabContext<'a>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<R>> + Send + 'a>,
        >,
        R: Send + 'static,
    {
        let temp_name = format!("temp-{}", uuid::Uuid::new_v4());

        // Create temporary tab and mark it as temporary
        self.create_tab(temp_name.clone()).await?;
        self.mark_tab_temporary(&temp_name).await;

        // Use with_tab for the operation (gets locking for free)
        let result = self.with_tab(&temp_name, operation).await;

        // Clean up temporary tab
        if let Err(e) = self.close_tab(&temp_name).await {
            warn!("Failed to close temporary tab '{}': {}", temp_name, e);
        }

        result
    }

    /// Mark a tab as temporary (will be cleaned up after use)
    pub async fn mark_tab_temporary(&self, name: &str) {
        let mut temp_tabs = self.temporary_tabs.lock().await;
        temp_tabs.insert(name.to_string());
    }

    /// Check if a tab is temporary
    pub async fn is_tab_temporary(&self, name: &str) -> bool {
        let temp_tabs = self.temporary_tabs.lock().await;
        temp_tabs.contains(name)
    }

    /// Clean up a temporary tab if it exists
    pub async fn cleanup_if_temporary(&self, name: &str) -> Result<()> {
        if self.is_tab_temporary(name).await {
            self.close_tab(name).await?;
            let mut temp_tabs = self.temporary_tabs.lock().await;
            temp_tabs.remove(name);
        }
        Ok(())
    }

    /// Check if a tab exists
    pub async fn has_tab(&self, name: &str) -> bool {
        let tabs = self.tabs.lock().await;
        tabs.contains_key(name)
    }

    /// Get browser type
    pub fn browser_type(&self) -> BrowserType {
        self.browser_type
    }

    /// Shutdown the browser manager and close the browser
    pub async fn shutdown(self) -> Result<()> {
        // Close all tabs first
        let tabs = self.tabs.lock().await;
        let handles: Vec<_> = tabs.values().cloned().collect();
        drop(tabs);

        for handle in handles {
            // Switch to the window then close it
            if let Err(e) = self.browser.client.switch_to_window(handle).await {
                tracing::debug!("Error switching to window during shutdown: {}", e);
                continue;
            }
            if let Err(e) = self.browser.client.close_window().await {
                tracing::debug!("Error closing window during shutdown: {}", e);
            }
        }

        // Close the browser
        self.browser.close().await?;
        Ok(())
    }
}

// Note: We don't implement Drop for BrowserManager because we have an explicit
// shutdown() method that must be called to properly clean up browser resources.
// The daemon is responsible for calling shutdown() when it terminates.

// The daemon will manage its own instance of BrowserManager
// This simplifies the architecture and avoids global state

#[cfg(test)]
#[path = "browser_manager_test.rs"]
mod browser_manager_test;
