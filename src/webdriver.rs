use anyhow::{Context, Result};
use fantoccini::{Client, ClientBuilder, Locator};
use serde_json::json;
use tracing::{debug, info};

use crate::profile::ProfileManager;
use crate::types::{
    ElementInfo, ElementMetadata, InspectionDepth, LayoutInfo, Position, Size, ViewportSize,
};
use crate::webdriver_manager::GLOBAL_WEBDRIVER_MANAGER;

use std::sync::Arc;
use tokio::sync::Mutex;

/// Browser instance for WebDriver automation
pub struct Browser {
    pub(crate) client: Client,
    browser_type: BrowserType,
    console_logs: Arc<Mutex<Vec<ConsoleMessage>>>,
}

/// Console message captured from the browser
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsoleMessage {
    /// Log level (log, warn, error, info)
    pub level: String,
    /// The console message text
    pub message: String,
    /// Timestamp when the message was logged
    pub timestamp: String,
}

/// Supported browser types
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BrowserType {
    /// Mozilla Firefox
    Firefox,
    /// Google Chrome/Chromium
    Chrome,
}

impl std::str::FromStr for BrowserType {
    type Err = anyhow::Error;

    /// Parse browser type from string (case-insensitive)
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "firefox" => Ok(BrowserType::Firefox),
            "chrome" | "chromium" => Ok(BrowserType::Chrome),
            _ => anyhow::bail!("Unsupported browser: {}", s),
        }
    }
}

impl BrowserType {
    /// Get the WebDriver URL for this browser type
    pub fn get_webdriver_url(&self) -> String {
        match self {
            BrowserType::Firefox => "http://localhost:4444".to_string(),
            BrowserType::Chrome => {
                // Try to detect chromedriver port from log files
                for log_file in &["/tmp/chromedriver_new.log", "/tmp/chromedriver.log"] {
                    if let Ok(log) = std::fs::read_to_string(log_file)
                        && let Some(line) = log
                            .lines()
                            .find(|l| l.contains("ChromeDriver was started successfully on port"))
                        && let Some(port) = line
                            .split("port ")
                            .nth(1)
                            .and_then(|s| s.trim_end_matches('.').parse::<u16>().ok())
                    {
                        return format!("http://localhost:{port}");
                    }
                }
                "http://localhost:9515".to_string()
            }
        }
    }
}

impl Browser {
    /// Create a new browser instance
    ///
    /// # Arguments
    /// * `browser_type` - Firefox or Chrome
    /// * `profile` - Optional profile name for session persistence
    /// * `viewport` - Optional viewport dimensions
    /// * `headless` - Whether to run in headless mode
    pub async fn new(
        browser_type: BrowserType,
        profile: Option<String>,
        viewport: Option<ViewportSize>,
        headless: bool,
    ) -> Result<Self> {
        info!("Connecting to {:?} WebDriver", browser_type);

        // Ensure WebDriver is running (will auto-start if needed)
        let webdriver_url = GLOBAL_WEBDRIVER_MANAGER
            .ensure_driver(&browser_type)
            .await?;

        // Double-check it's really running (should always be true now)
        if !Self::is_webdriver_running(&webdriver_url).await {
            let driver_name = match browser_type {
                BrowserType::Firefox => "geckodriver",
                BrowserType::Chrome => "chromedriver",
            };

            anyhow::bail!(
                "Cannot connect to {} WebDriver at {}.\n\
                Please ensure {} is running:\n\
                  For Firefox: geckodriver --port 4444\n\
                  For Chrome: chromedriver --port 9515\n\n\
                Install instructions:\n\
                  macOS: brew install {}\n\
                  Linux: Download from https://github.com/mozilla/geckodriver/releases\n\
                  Windows: Download and add to PATH",
                driver_name,
                webdriver_url,
                driver_name,
                driver_name
            );
        }

        // Get or create profile path
        let profile_path = if let Some(profile_name) = profile {
            // When profile is specified with daemon/tabs, use temp dir for isolation only
            // This gives each profile its own cookie space without disk persistence
            let temp_dir = tempfile::Builder::new()
                .prefix(&format!("webprobe-{profile_name}-"))
                .tempdir()?;
            #[allow(deprecated)]
            temp_dir.into_path() // We want to keep the directory
        } else {
            // For Chrome, always use a unique temp directory to avoid conflicts
            // Chrome is more strict about profile directory usage
            if matches!(browser_type, BrowserType::Chrome) {
                let temp_dir = tempfile::Builder::new()
                    .prefix("webprobe-chrome-")
                    .tempdir()?;
                #[allow(deprecated)]
                temp_dir.into_path()
            } else {
                // Use temporary profile for Firefox
                let manager = ProfileManager::new()?;
                let browser_name = format!("{browser_type:?}").to_lowercase();
                manager.create_temporary_profile(&browser_name)?
            }
        };

        let mut caps = serde_json::Map::new();

        match &browser_type {
            BrowserType::Firefox => {
                let mut firefox_opts = serde_json::Map::new();
                let mut args = Vec::new();

                if headless {
                    args.push("--headless".to_string());
                }

                if let Some(vp) = &viewport {
                    args.push(format!("--width={}", vp.width));
                    args.push(format!("--height={}", vp.height));
                }

                firefox_opts.insert("args".to_string(), json!(args));
                caps.insert("moz:firefoxOptions".to_string(), json!(firefox_opts));
            }
            BrowserType::Chrome => {
                let mut chrome_opts = serde_json::Map::new();
                let mut args = vec!["--no-sandbox".to_string()];

                if headless {
                    // Chrome 112+ changed headless behavior
                    // Use --headless for true headless (no window at all)
                    // Note: Chrome 139 supports this syntax
                    args.push("--headless=new".to_string());
                    // Disable GPU for headless
                    args.push("--disable-gpu".to_string());
                    // Prevent shared memory issues
                    args.push("--disable-dev-shm-usage".to_string());
                }

                if let Some(vp) = &viewport {
                    args.push(format!("--window-size={},{}", vp.width, vp.height));
                }

                args.push(format!("--user-data-dir={}", profile_path.display()));

                chrome_opts.insert("args".to_string(), json!(args));
                caps.insert("goog:chromeOptions".to_string(), json!(chrome_opts));
            }
        }

        debug!("Connecting to WebDriver at {}", webdriver_url);

        let client = match ClientBuilder::rustls()
            .capabilities(caps.clone())
            .connect(&webdriver_url)
            .await
        {
            Ok(client) => client,
            Err(e) => {
                let error_str = e.to_string();
                // Check for common error patterns
                if error_str.contains("Session is already started")
                    || error_str.contains("session not created")
                {
                    // WebDriver is in a bad state, try to recover
                    info!("WebDriver appears to be in a bad state, attempting recovery...");

                    // Kill the existing driver
                    GLOBAL_WEBDRIVER_MANAGER.kill_driver(&browser_type);

                    // Wait a bit for it to fully terminate
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                    // Try to restart it
                    let new_url = GLOBAL_WEBDRIVER_MANAGER
                        .ensure_driver(&browser_type)
                        .await
                        .context("Failed to restart WebDriver after recovery")?;

                    // Try connecting again
                    ClientBuilder::rustls()
                        .capabilities(caps)
                        .connect(&new_url)
                        .await
                        .context("Failed to connect to WebDriver after restart")?
                } else {
                    return Err(e).context("Failed to connect to WebDriver");
                }
            }
        };

        // Set viewport size after connection if specified
        if let Some(vp) = viewport {
            debug!("Setting viewport to {}x{}", vp.width, vp.height);
            if let Err(e) = client.set_window_size(vp.width, vp.height).await {
                debug!("Note: Could not set window size: {}", e);
                // Continue anyway - viewport setting is best-effort
            }
        }

        let browser = Browser {
            client,
            browser_type,
            console_logs: Arc::new(Mutex::new(Vec::new())),
        };

        // Set up console log capture
        browser.setup_console_capture().await?;

        Ok(browser)
    }

    async fn is_webdriver_running(url: &str) -> bool {
        // Try to connect to the WebDriver status endpoint
        let status_url = format!("{}/status", url);

        match reqwest::get(&status_url).await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn setup_console_capture(&self) -> Result<()> {
        // Inject JavaScript to capture console messages
        let capture_script = r#"
            (function() {
                if (window.__webprobe_console_capture) return;
                window.__webprobe_console_capture = true;
                window.__webprobe_console_logs = [];
                
                const originalLog = console.log;
                const originalError = console.error;
                const originalWarn = console.warn;
                const originalInfo = console.info;
                
                function captureLog(level, args) {
                    const message = Array.from(args).map(arg => {
                        if (typeof arg === 'object') {
                            try {
                                return JSON.stringify(arg);
                            } catch (e) {
                                return String(arg);
                            }
                        }
                        return String(arg);
                    }).join(' ');
                    
                    window.__webprobe_console_logs.push({
                        level: level,
                        message: message,
                        timestamp: new Date().toISOString()
                    });
                    
                    // Keep only last 1000 messages
                    if (window.__webprobe_console_logs.length > 1000) {
                        window.__webprobe_console_logs.shift();
                    }
                }
                
                console.log = function(...args) {
                    captureLog('log', args);
                    originalLog.apply(console, args);
                };
                
                console.error = function(...args) {
                    captureLog('error', args);
                    originalError.apply(console, args);
                };
                
                console.warn = function(...args) {
                    captureLog('warn', args);
                    originalWarn.apply(console, args);
                };
                
                console.info = function(...args) {
                    captureLog('info', args);
                    originalInfo.apply(console, args);
                };
                
                // Capture unhandled errors
                window.addEventListener('error', function(event) {
                    captureLog('error', [`Uncaught ${event.error || event.message} at ${event.filename}:${event.lineno}:${event.colno}`]);
                });
                
                window.addEventListener('unhandledrejection', function(event) {
                    captureLog('error', [`Unhandled Promise Rejection: ${event.reason}`]);
                });
            })();
        "#;

        // Execute the script (ignore errors as it might fail on some pages)
        let _ = self.client.execute(capture_script, vec![]).await;

        Ok(())
    }

    pub async fn get_console_logs(&self) -> Result<Vec<ConsoleMessage>> {
        // Try to get logs from the page
        let script = "return window.__webprobe_console_logs || [];";

        match self.client.execute(script, vec![]).await {
            Ok(value) => {
                // Parse the JSON value into our ConsoleMessage struct
                if let Ok(logs) = serde_json::from_value::<Vec<ConsoleMessage>>(value) {
                    // Store logs in our internal buffer
                    let mut stored_logs = self.console_logs.lock().await;
                    stored_logs.extend_from_slice(&logs);

                    // Return all logs
                    Ok(stored_logs.clone())
                } else {
                    Ok(Vec::new())
                }
            }
            Err(_) => {
                // If we can't get logs from the page, return what we have stored
                Ok(self.console_logs.lock().await.clone())
            }
        }
    }

    #[allow(dead_code)]
    pub async fn clear_console_logs(&self) -> Result<()> {
        // Clear stored logs
        self.console_logs.lock().await.clear();

        // Clear logs on the page
        let _ = self
            .client
            .execute("window.__webprobe_console_logs = [];", vec![])
            .await;

        Ok(())
    }

    pub async fn goto(&self, url: &str) -> Result<()> {
        info!("Navigating to {}", url);

        // Try to inject console capture before navigation for Chrome
        // This may help capture early logs
        if matches!(self.browser_type, BrowserType::Chrome) {
            let _ = self.setup_console_capture().await;
        }

        self.client.goto(url).await?;

        // Wait for the page to be ready
        // This helps avoid stale element references
        let wait_script = r#"
            return document.readyState === 'complete';
        "#;

        // Try waiting for page to be ready (with timeout)
        for _ in 0..20 {
            // Max 2 seconds
            match self.client.execute(wait_script, vec![]).await {
                Ok(val) if val.as_bool().unwrap_or(false) => {
                    break;
                }
                _ => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }

        Ok(())
    }

    /// Get the current URL - useful for health checks
    pub async fn get_current_url(&self) -> Result<String> {
        Ok(self.client.current_url().await?.to_string())
    }

    #[allow(dead_code)]
    pub async fn navigate_and_setup(&self, url: &str) -> Result<()> {
        self.goto(url).await?;

        // Always inject console capture after navigation
        // This ensures it's available even if pre-injection failed
        self.setup_console_capture().await?;

        // For file:// URLs with inline scripts, wait a bit for scripts to execute
        if url.starts_with("file://") {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        Ok(())
    }

    pub async fn inspect_element_with_console(
        &self,
        url: &str,
        selector: &str,
        _depth: InspectionDepth,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
        capture_console: bool,
    ) -> Result<(Vec<ElementInfo>, Option<Vec<ConsoleMessage>>)> {
        let elements = self
            .inspect_element(url, selector, _depth, all, index, expect_one)
            .await?;

        let console_logs = if capture_console {
            // Wait a bit for any async console logs
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            Some(self.get_console_logs().await?)
        } else {
            None
        };

        Ok((elements, console_logs))
    }

    pub async fn inspect_element(
        &self,
        url: &str,
        selector: &str,
        _depth: InspectionDepth,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
    ) -> Result<Vec<ElementInfo>> {
        // Navigate if URL is provided
        if !url.is_empty() {
            debug!("Navigating to: {}", url);
            self.goto(url).await?;
        }

        debug!("Finding elements with selector: {}", selector);

        // Find all matching elements
        let elements = self
            .client
            .find_all(Locator::Css(selector))
            .await
            .context(format!("No elements found matching selector: {}", selector))?;

        if elements.is_empty() {
            anyhow::bail!("No elements found matching selector: {}", selector);
        }

        let total_count = elements.len();

        // Warn if multiple elements exist but we're not returning all
        if total_count > 1 && !all && index.is_none() {
            info!(
                "Warning: {} elements match '{}'. Returning first. Use --all to see all.",
                total_count, selector
            );
        }

        // Handle expect_one flag
        if expect_one && elements.len() > 1 {
            anyhow::bail!(
                "Expected exactly one element matching '{}', but found {}",
                selector,
                elements.len()
            );
        }

        // Determine which elements to process
        let elements_to_process = if let Some(idx) = index {
            // Specific index requested
            if idx >= elements.len() {
                anyhow::bail!(
                    "Index {} out of bounds. Found {} elements matching '{}'",
                    idx,
                    elements.len(),
                    selector
                );
            }
            vec![&elements[idx]]
        } else if all {
            // All elements requested
            elements.iter().collect()
        } else {
            // Default: first element only
            vec![&elements[0]]
        };

        // Process each element
        let mut results = Vec::new();
        for (i, element) in elements_to_process.iter().enumerate() {
            let rect = element.rectangle().await?;
            let text_content = element.text().await.ok();
            let is_displayed = element.is_displayed().await?;
            let tag_name = element.tag_name().await?;

            let computed_styles = json!({
                "display": if is_displayed { "block" } else { "none" },
                "tag": tag_name,
                "index": if all || index.is_some() { Some(i) } else { None },
            });

            // For children count, we need a more specific selector
            let element_id = element.attr("id").await.ok().flatten();
            let children_count = if let Some(id) = element_id {
                self.client
                    .find_all(Locator::Css(&format!("#{} > *", id)))
                    .await
                    .map(|c| c.len())
                    .unwrap_or(0)
            } else {
                // Fallback: can't easily count children without unique identifier
                0
            };

            // Add metadata if there are multiple matches but we're returning only one
            let metadata = if total_count > 1 && !all {
                Some(ElementMetadata {
                    total_matches: total_count,
                    returned_index: index.unwrap_or(0),
                    warning: if index.is_none() {
                        Some(format!(
                            "{} elements match '{}'. Showing first. Use --all to see all.",
                            total_count, selector
                        ))
                    } else {
                        None
                    },
                })
            } else {
                None
            };

            results.push(ElementInfo {
                selector: selector.to_string(),
                browser: format!("{:?}", self.browser_type),
                position: Position {
                    x: rect.0,
                    y: rect.1,
                    unit: "px".to_string(),
                },
                size: Size {
                    width: rect.2,
                    height: rect.3,
                    unit: "px".to_string(),
                },
                computed_styles,
                text_content,
                children_count,
                metadata,
            });
        }

        info!("Found {} element(s) matching selector", results.len());
        Ok(results)
    }

    pub async fn click_element(
        &self,
        url: &str,
        selector: &str,
        index: Option<usize>,
    ) -> Result<()> {
        // Navigate if URL is provided
        if !url.is_empty() {
            self.goto(url).await?;
        }

        debug!("Finding element with selector: {}", selector);

        if let Some(idx) = index {
            // Click specific index
            let elements = self
                .client
                .find_all(Locator::Css(selector))
                .await
                .context(format!("No elements found matching selector: {}", selector))?;

            if idx >= elements.len() {
                anyhow::bail!(
                    "Index {} out of bounds. Found {} elements matching '{}'",
                    idx,
                    elements.len(),
                    selector
                );
            }

            info!("Clicking element at index {}", idx);
            elements[idx].click().await?;
        } else {
            // Click first element
            let element = self
                .client
                .find(Locator::Css(selector))
                .await
                .context(format!("Element not found: {}", selector))?;

            info!("Clicking element");
            element.click().await?;
        }

        Ok(())
    }

    pub async fn type_text(
        &self,
        url: &str,
        selector: &str,
        text: &str,
        clear: bool,
    ) -> Result<()> {
        // Navigate if URL is provided
        if !url.is_empty() {
            self.goto(url).await?;
        }

        debug!("Finding element with selector: {}", selector);
        let element = self
            .client
            .find(Locator::Css(selector))
            .await
            .context(format!("Element not found: {}", selector))?;

        if clear {
            info!("Clearing field");
            element.clear().await?;
        }

        info!("Typing text into element");
        element.send_keys(text).await?;

        Ok(())
    }

    pub async fn scroll(
        &self,
        url: &str,
        selector: Option<&str>,
        by_x: i32,
        by_y: i32,
        to: Option<&str>,
    ) -> Result<()> {
        // Navigate if URL is provided
        if !url.is_empty() {
            self.goto(url).await?;
        }

        let script = if let Some(to_pos) = to {
            // Scroll to specific position
            match to_pos {
                "top" => {
                    if let Some(sel) = selector {
                        format!("document.querySelector('{}').scrollTo(0, 0);", sel)
                    } else {
                        "window.scrollTo(0, 0);".to_string()
                    }
                }
                "bottom" => {
                    if let Some(sel) = selector {
                        format!(
                            "var el = document.querySelector('{}'); el.scrollTo(0, el.scrollHeight);",
                            sel
                        )
                    } else {
                        "window.scrollTo(0, document.body.scrollHeight);".to_string()
                    }
                }
                pos if pos.contains(',') => {
                    let parts: Vec<&str> = pos.split(',').collect();
                    if parts.len() == 2 {
                        let x = parts[0].trim();
                        let y = parts[1].trim();
                        if let Some(sel) = selector {
                            format!("document.querySelector('{}').scrollTo({}, {});", sel, x, y)
                        } else {
                            format!("window.scrollTo({}, {});", x, y)
                        }
                    } else {
                        anyhow::bail!("Invalid position format. Use 'x,y' or 'top'/'bottom'");
                    }
                }
                _ => anyhow::bail!("Invalid scroll position: {}", to_pos),
            }
        } else {
            // Scroll by relative amount
            if let Some(sel) = selector {
                format!(
                    "document.querySelector('{}').scrollBy({}, {});",
                    sel, by_x, by_y
                )
            } else {
                format!("window.scrollBy({}, {});", by_x, by_y)
            }
        };

        debug!("Executing scroll: {}", script);
        self.client.execute(&script, vec![]).await?;

        info!("Scroll completed");
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn execute(
        &self,
        script: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.client
            .execute(script, args)
            .await
            .context("Failed to execute script")
    }

    pub async fn execute_javascript(
        &self,
        url: Option<&str>,
        code: &str,
    ) -> Result<serde_json::Value> {
        // Navigate to URL if provided
        if let Some(url) = url {
            info!("Navigating to {}", url);
            self.client.goto(url).await?;
        }

        debug!("Executing JavaScript: {}", code);

        // Execute the JavaScript and get the result
        let result = self
            .client
            .execute(code, vec![])
            .await
            .context("Failed to execute JavaScript")?;

        // The result is already a serde_json::Value
        let json_value = result;

        info!("JavaScript execution completed");
        Ok(json_value)
    }

    pub async fn analyze_layout(
        &self,
        url: &str,
        selector: &str,
        depth: u8,
        max_elements: usize,
        wait_stable: u64,
        detect_shadow: bool,
    ) -> Result<LayoutInfo> {
        // Navigate if URL is provided
        if !url.is_empty() {
            info!("Navigating to {} for layout analysis", url);
            self.client.goto(url).await?;
        }

        // Wait for layout to stabilize
        if wait_stable > 0 {
            info!("Waiting {}ms for layout to stabilize", wait_stable);
            tokio::time::sleep(tokio::time::Duration::from_millis(wait_stable)).await;
        }

        // JavaScript to analyze layout with performance limits
        let analysis_script = r#"
        function analyzeLayout(selector, maxDepth, maxElements, detectShadow) {
            let elementCount = 0;
            const warnings = [];
            
            function getBoxModel(element) {
                const styles = window.getComputedStyle(element);
                const rect = element.getBoundingClientRect();
                
                return {
                    bounds: {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height
                    },
                    box_model: {
                        margin: {
                            top: parseFloat(styles.marginTop) || 0,
                            right: parseFloat(styles.marginRight) || 0,
                            bottom: parseFloat(styles.marginBottom) || 0,
                            left: parseFloat(styles.marginLeft) || 0
                        },
                        border: {
                            top: parseFloat(styles.borderTopWidth) || 0,
                            right: parseFloat(styles.borderRightWidth) || 0,
                            bottom: parseFloat(styles.borderBottomWidth) || 0,
                            left: parseFloat(styles.borderLeftWidth) || 0
                        },
                        padding: {
                            top: parseFloat(styles.paddingTop) || 0,
                            right: parseFloat(styles.paddingRight) || 0,
                            bottom: parseFloat(styles.paddingBottom) || 0,
                            left: parseFloat(styles.paddingLeft) || 0
                        },
                        content: {
                            width: rect.width - 
                                   parseFloat(styles.paddingLeft) - 
                                   parseFloat(styles.paddingRight) -
                                   parseFloat(styles.borderLeftWidth) -
                                   parseFloat(styles.borderRightWidth),
                            height: rect.height - 
                                    parseFloat(styles.paddingTop) - 
                                    parseFloat(styles.paddingBottom) -
                                    parseFloat(styles.borderTopWidth) -
                                    parseFloat(styles.borderBottomWidth)
                        }
                    },
                    computed_styles: {
                        display: styles.display,
                        position: styles.position,
                        zIndex: styles.zIndex,
                        overflow: styles.overflow,
                        float: styles.float,
                        clear: styles.clear,
                        boxSizing: styles.boxSizing,
                        flexDirection: styles.flexDirection,
                        justifyContent: styles.justifyContent,
                        alignItems: styles.alignItems,
                        gap: styles.gap
                    },
                    is_visible: rect.width > 0 && rect.height > 0 && 
                               styles.display !== 'none' && 
                               styles.visibility !== 'hidden'
                };
            }
            
            function analyzeElement(element, currentDepth) {
                if (!element || elementCount >= maxElements || currentDepth > maxDepth) {
                    return null;
                }
                
                elementCount++;
                
                const result = {
                    selector: selector,
                    tag: element.tagName.toLowerCase(),
                    classes: Array.from(element.classList),
                    id: element.id || null,
                    ...getBoxModel(element),
                    children: [],
                    warnings: [],
                    element_count: 1,
                    truncated: false
                };
                
                // Detect shadow DOM
                if (detectShadow && element.shadowRoot) {
                    result.warnings.push('Element has shadow DOM (not analyzed)');
                }
                
                // Detect iframes
                if (element.tagName === 'IFRAME') {
                    result.warnings.push('Element is an iframe (content not analyzed)');
                }
                
                // Analyze children if within depth
                if (currentDepth < maxDepth) {
                    const children = Array.from(element.children);
                    for (let child of children) {
                        if (elementCount >= maxElements) {
                            result.truncated = true;
                            result.warnings.push(`Analysis truncated at ${maxElements} elements`);
                            break;
                        }
                        const childAnalysis = analyzeElement(child, currentDepth + 1);
                        if (childAnalysis) {
                            result.children.push(childAnalysis);
                            result.element_count += childAnalysis.element_count;
                        }
                    }
                }
                
                return result;
            }
            
            const element = document.querySelector(selector);
            if (!element) {
                throw new Error('Element not found: ' + selector);
            }
            
            const analysis = analyzeElement(element, 0);
            analysis.warnings = warnings;
            return analysis;
        }
        
        return analyzeLayout(arguments[0], arguments[1], arguments[2], arguments[3]);
        "#;

        debug!(
            "Running layout analysis with depth={}, max_elements={}",
            depth, max_elements
        );

        let result = self
            .client
            .execute(
                analysis_script,
                vec![
                    json!(selector),
                    json!(depth),
                    json!(max_elements),
                    json!(detect_shadow),
                ],
            )
            .await
            .context("Failed to analyze layout")?;

        // Convert the result to LayoutInfo
        let layout: LayoutInfo =
            serde_json::from_value(result).context("Failed to parse layout analysis")?;

        info!(
            "Layout analysis complete: {} elements analyzed",
            layout.element_count
        );

        Ok(layout)
    }

    pub async fn analyze_context(
        &self,
        url: &str,
        selector: &str,
        focus: &str,
        proximity: u32,
        index: Option<usize>,
    ) -> Result<serde_json::Value> {
        // Navigate if URL is provided
        if !url.is_empty() {
            info!("Navigating to {} for context analysis", url);
            self.client.goto(url).await?;
        }

        // Wait a bit for page to stabilize
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let script = match focus {
            "spacing" => self.build_spacing_script(selector, proximity, index),
            "wrapping" => self.build_wrapping_script(selector, index),
            "anomalies" => self.build_anomalies_script(proximity),
            "all" => self.build_comprehensive_script(selector, proximity, index),
            _ => self.build_comprehensive_script(selector, proximity, index),
        };

        debug!("Running {} analysis", focus);

        let result = self
            .client
            .execute(&script, vec![])
            .await
            .context(format!("Failed to analyze {} context", focus))?;

        info!("Context analysis complete");
        Ok(result)
    }

    fn build_spacing_script(&self, selector: &str, proximity: u32, index: Option<usize>) -> String {
        let target_selection = if let Some(idx) = index {
            format!(
                "const elements = document.querySelectorAll('{}');
            if (elements.length <= {}) return {{ error: 'Element at index {} not found' }};
            const target = elements[{}];",
                selector, idx, idx, idx
            )
        } else {
            format!(
                "const target = document.querySelector('{}');
            if (!target) return {{ error: 'Element not found: {}' }};",
                selector, selector
            )
        };

        format!(
            r#"
        return (function() {{
            {}
            
            const targetRect = target.getBoundingClientRect();
            const targetStyles = window.getComputedStyle(target);
            
            // Get target's box model
            const targetBox = {{
                margin: {{
                    top: parseFloat(targetStyles.marginTop) || 0,
                    right: parseFloat(targetStyles.marginRight) || 0,
                    bottom: parseFloat(targetStyles.marginBottom) || 0,
                    left: parseFloat(targetStyles.marginLeft) || 0
                }},
                padding: {{
                    top: parseFloat(targetStyles.paddingTop) || 0,
                    right: parseFloat(targetStyles.paddingRight) || 0,
                    bottom: parseFloat(targetStyles.paddingBottom) || 0,
                    left: parseFloat(targetStyles.paddingLeft) || 0
                }},
                border: {{
                    top: parseFloat(targetStyles.borderTopWidth) || 0,
                    right: parseFloat(targetStyles.borderRightWidth) || 0,
                    bottom: parseFloat(targetStyles.borderBottomWidth) || 0,
                    left: parseFloat(targetStyles.borderLeftWidth) || 0
                }}
            }};
            
            // Find adjacent elements
            const allElements = Array.from(document.querySelectorAll('*'));
            const adjacentElements = [];
            
            for (let el of allElements) {{
                if (el === target || target.contains(el) || el.contains(target)) continue;
                
                const rect = el.getBoundingClientRect();
                const styles = window.getComputedStyle(el);
                
                // Check if element is above
                if (rect.bottom <= targetRect.top && 
                    Math.abs(rect.bottom - targetRect.top) < {}) {{
                    adjacentElements.push({{
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (el.className && typeof el.className === 'string' ? '.' + el.className.split(' ').join('.') : ''),
                        position: 'above',
                        bounds: {{
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            bottom: rect.bottom
                        }},
                        margin_bottom: parseFloat(styles.marginBottom) || 0,
                        actual_gap: targetRect.top - rect.bottom,
                        margin_collapsed: (targetRect.top - rect.bottom) < (parseFloat(styles.marginBottom) + targetBox.margin.top)
                    }});
                }}
                
                // Check if element is below
                if (rect.top >= targetRect.bottom && 
                    Math.abs(rect.top - targetRect.bottom) < {}) {{
                    adjacentElements.push({{
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (el.className && typeof el.className === 'string' ? '.' + el.className.split(' ').join('.') : ''),
                        position: 'below',
                        bounds: {{
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            top: rect.top
                        }},
                        margin_top: parseFloat(styles.marginTop) || 0,
                        actual_gap: rect.top - targetRect.bottom,
                        margin_collapsed: (rect.top - targetRect.bottom) < (targetBox.margin.bottom + parseFloat(styles.marginTop))
                    }});
                }}
                
                // Check if element is to the left
                if (rect.right <= targetRect.left && 
                    Math.abs(rect.right - targetRect.left) < {}) {{
                    adjacentElements.push({{
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (el.className && typeof el.className === 'string' ? '.' + el.className.split(' ').join('.') : ''),
                        position: 'left',
                        bounds: {{
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            right: rect.right
                        }},
                        margin_right: parseFloat(styles.marginRight) || 0,
                        actual_gap: targetRect.left - rect.right
                    }});
                }}
                
                // Check if element is to the right
                if (rect.left >= targetRect.right && 
                    Math.abs(rect.left - targetRect.right) < {}) {{
                    adjacentElements.push({{
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (el.className && typeof el.className === 'string' ? '.' + el.className.split(' ').join('.') : ''),
                        position: 'right',
                        bounds: {{
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            left: rect.left
                        }},
                        margin_left: parseFloat(styles.marginLeft) || 0,
                        actual_gap: rect.left - targetRect.right
                    }});
                }}
            }}
            
            // Get parent padding
            const parent = target.parentElement;
            const parentStyles = parent ? window.getComputedStyle(parent) : null;
            const parentPadding = parent ? {{
                top: parseFloat(parentStyles.paddingTop) || 0,
                right: parseFloat(parentStyles.paddingRight) || 0,
                bottom: parseFloat(parentStyles.paddingBottom) || 0,
                left: parseFloat(parentStyles.paddingLeft) || 0
            }} : null;
            
            // Check for pseudo elements
            const beforeStyles = window.getComputedStyle(target, ':before');
            const afterStyles = window.getComputedStyle(target, ':after');
            
            const pseudoElements = {{
                before: beforeStyles.content !== 'none' ? {{
                    content: beforeStyles.content,
                    height: parseFloat(beforeStyles.height) || 'auto',
                    width: parseFloat(beforeStyles.width) || 'auto',
                    display: beforeStyles.display
                }} : null,
                after: afterStyles.content !== 'none' ? {{
                    content: afterStyles.content,
                    height: parseFloat(afterStyles.height) || 'auto',
                    width: parseFloat(afterStyles.width) || 'auto',
                    display: afterStyles.display
                }} : null
            }};
            
            return {{
                target: {{
                    selector: '{}',
                    bounds: {{
                        x: targetRect.x,
                        y: targetRect.y,
                        width: targetRect.width,
                        height: targetRect.height
                    }},
                    box: targetBox,
                    display: targetStyles.display,
                    position: targetStyles.position
                }},
                spacing_context: {{
                    adjacent_elements: adjacentElements,
                    parent: parent ? {{
                        selector: parent.tagName.toLowerCase() + (parent.id ? '#' + parent.id : ''),
                        padding: parentPadding
                    }} : null,
                    pseudo_elements: pseudoElements
                }},
                element_count: adjacentElements.length + 1
            }};
        }})();
        "#,
            target_selection, proximity, proximity, proximity, proximity, selector
        )
    }

    fn build_wrapping_script(&self, selector: &str, index: Option<usize>) -> String {
        let target_selection = if let Some(idx) = index {
            format!(
                "const elements = document.querySelectorAll('{}');
            if (elements.length <= {}) return {{ error: 'Container at index {} not found' }};
            const container = elements[{}];",
                selector, idx, idx, idx
            )
        } else {
            format!(
                "const container = document.querySelector('{}');
            if (!container) return {{ error: 'Container not found: {}' }};",
                selector, selector
            )
        };

        format!(
            r#"
        return (function() {{
            {}
            
            const containerRect = container.getBoundingClientRect();
            const containerStyles = window.getComputedStyle(container);
            
            // Get container properties
            const containerInfo = {{
                selector: '{}',
                display: containerStyles.display,
                flexWrap: containerStyles.flexWrap,
                flexDirection: containerStyles.flexDirection,
                gridTemplateColumns: containerStyles.gridTemplateColumns,
                gap: containerStyles.gap,
                columnGap: containerStyles.columnGap,
                rowGap: containerStyles.rowGap,
                width: containerRect.width,
                padding: {{
                    left: parseFloat(containerStyles.paddingLeft) || 0,
                    right: parseFloat(containerStyles.paddingRight) || 0,
                    top: parseFloat(containerStyles.paddingTop) || 0,
                    bottom: parseFloat(containerStyles.paddingBottom) || 0
                }},
                available_width: containerRect.width - 
                    (parseFloat(containerStyles.paddingLeft) || 0) - 
                    (parseFloat(containerStyles.paddingRight) || 0)
            }};
            
            // Analyze children
            const children = Array.from(container.children);
            const childrenInfo = [];
            let currentRowY = null;
            let currentRowWidth = 0;
            let rowCount = 1;
            
            children.forEach((child, index) => {{
                const rect = child.getBoundingClientRect();
                const styles = window.getComputedStyle(child);
                
                // Detect row wrapping
                let wrapped = false;
                if (currentRowY !== null && Math.abs(rect.y - currentRowY) > 5) {{
                    wrapped = true;
                    rowCount++;
                    currentRowY = rect.y;
                    currentRowWidth = 0;
                }}
                if (currentRowY === null) {{
                    currentRowY = rect.y;
                }}
                
                const childWidth = rect.width + 
                    (parseFloat(styles.marginLeft) || 0) + 
                    (parseFloat(styles.marginRight) || 0);
                
                currentRowWidth += childWidth;
                
                childrenInfo.push({{
                    index: index,
                    tag: child.tagName.toLowerCase(),
                    width: rect.width,
                    height: rect.height,
                    margin: {{
                        left: parseFloat(styles.marginLeft) || 0,
                        right: parseFloat(styles.marginRight) || 0,
                        top: parseFloat(styles.marginTop) || 0,
                        bottom: parseFloat(styles.marginBottom) || 0
                    }},
                    total_width: childWidth,
                    wrapped: wrapped,
                    row: rowCount,
                    position_in_row: wrapped ? 1 : childrenInfo.filter(c => c.row === rowCount).length + 1
                }});
            }});
            
            // Calculate total width needed
            const totalChildrenWidth = childrenInfo
                .filter(c => c.row === 1)
                .reduce((sum, c) => sum + c.total_width, 0);
            
            // Detect gap/spacing
            let estimatedGap = 0;
            if (childrenInfo.length > 1) {{
                const firstChild = container.children[0].getBoundingClientRect();
                const secondChild = container.children[1].getBoundingClientRect();
                if (Math.abs(firstChild.y - secondChild.y) < 5) {{
                    estimatedGap = secondChild.left - firstChild.right;
                }}
            }}
            
            return {{
                container: containerInfo,
                children: {{
                    total_count: children.length,
                    rows: rowCount,
                    per_row: childrenInfo.filter(c => c.row === 1).length,
                    child_analysis: childrenInfo
                }},
                width_calculation: {{
                    container_available: containerInfo.available_width,
                    total_children_width: totalChildrenWidth,
                    estimated_gap: estimatedGap,
                    overflow: totalChildrenWidth - containerInfo.available_width,
                    fits: totalChildrenWidth <= containerInfo.available_width
                }},
                element_count: children.length + 1
            }};
        }})();
        "#,
            target_selection, selector
        )
    }

    fn build_anomalies_script(&self, _proximity: u32) -> String {
        r#"
        return (function() {{
            const viewport = {{
                width: window.innerWidth,
                height: window.innerHeight,
                scrollWidth: document.documentElement.scrollWidth,
                scrollHeight: document.documentElement.scrollHeight,
                hasHorizontalScroll: document.documentElement.scrollWidth > window.innerWidth,
                hasVerticalScroll: document.documentElement.scrollHeight > window.innerHeight
            }};
            
            const allElements = Array.from(document.querySelectorAll('*'));
            const anomalies = {{
                elements_with_unusual_properties: [],
                elements_beyond_viewport: [],
                invisible_elements: [],
                interaction_conflicts: [],
                contrast_issues: []
            }};
            
            const stats = {{
                total_elements: allElements.length,
                with_display_none: 0,
                with_visibility_hidden: 0,
                with_zero_dimensions: 0,
                outside_viewport: 0,
                with_negative_z_index: 0
            }};
            
            // Track z-index layers for conflict detection
            const zIndexLayers = new Map();
            
            allElements.forEach(el => {{
                const rect = el.getBoundingClientRect();
                const styles = window.getComputedStyle(el);
                const zIndex = styles.zIndex !== 'auto' ? parseInt(styles.zIndex) : 0;
                
                // Track statistics
                if (styles.display === 'none') stats.with_display_none++;
                if (styles.visibility === 'hidden') stats.with_visibility_hidden++;
                if (rect.width === 0 || rect.height === 0) stats.with_zero_dimensions++;
                if (rect.right < 0 || rect.left > viewport.width || 
                    rect.bottom < 0 || rect.top > viewport.height) stats.outside_viewport++;
                if (zIndex < 0) stats.with_negative_z_index++;
                
                // Detect unusual properties
                if (zIndex < 0 || 
                    (styles.opacity === '0' && styles.position === 'fixed') ||
                    (styles.position === 'absolute' && (rect.x < -100 || rect.y < -100))) {{
                    anomalies.elements_with_unusual_properties.push({{
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                        properties: {{
                            z_index: zIndex,
                            position: styles.position,
                            opacity: parseFloat(styles.opacity),
                            display: styles.display,
                            visibility: styles.visibility,
                            bounds: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }}
                        }}
                    }});
                }}
                
                // Detect elements beyond viewport
                if (rect.right > viewport.width && rect.width > 0) {{
                    anomalies.elements_beyond_viewport.push({{
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                        bounds: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }},
                        viewport_width: viewport.width,
                        overflow_amount: rect.right - viewport.width
                    }});
                }}
                
                // Detect invisible but space-taking elements
                if (el.textContent && el.textContent.trim().length > 0 && rect.width > 0 && rect.height > 0) {{
                    const isInvisible = styles.opacity === '0' || 
                                       styles.visibility === 'hidden' || 
                                       (styles.color === styles.backgroundColor && styles.backgroundColor !== 'transparent');
                    
                    if (isInvisible) {{
                        anomalies.invisible_elements.push({{
                            selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                            reason: styles.opacity === '0' ? 'opacity: 0' : 
                                   styles.visibility === 'hidden' ? 'visibility: hidden' : 
                                   'text color matches background',
                            text_length: el.textContent.trim().length,
                            bounds: {{ x: rect.x, y: rect.y, width: rect.width, height: rect.height }}
                        }});
                    }}
                    
                    // Check contrast
                    if (styles.color && styles.backgroundColor && styles.backgroundColor !== 'transparent') {{
                        anomalies.contrast_issues.push({{
                            selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                            color: styles.color,
                            background: styles.backgroundColor,
                            text_sample: el.textContent.substring(0, 50)
                        }});
                    }}
                }}
                
                // Track z-index layers for interaction conflict detection
                if (styles.position !== 'static' && rect.width > 0 && rect.height > 0) {{
                    if (!zIndexLayers.has(zIndex)) {{
                        zIndexLayers.set(zIndex, []);
                    }}
                    zIndexLayers.get(zIndex).push({{
                        element: el,
                        rect: rect,
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '')
                    }});
                }}
            }});
            
            // Detect interaction conflicts (overlapping clickable elements)
            const sortedZIndexes = Array.from(zIndexLayers.keys()).sort((a, b) => b - a);
            for (let i = 0; i < sortedZIndexes.length - 1; i++) {{
                const higherElements = zIndexLayers.get(sortedZIndexes[i]);
                for (let j = i + 1; j < sortedZIndexes.length; j++) {{
                    const lowerElements = zIndexLayers.get(sortedZIndexes[j]);
                    
                    higherElements.forEach(higher => {{
                        lowerElements.forEach(lower => {{
                            // Check if they overlap
                            if (higher.rect.left < lower.rect.right &&
                                higher.rect.right > lower.rect.left &&
                                higher.rect.top < lower.rect.bottom &&
                                higher.rect.bottom > lower.rect.top) {{
                                
                                // Check if lower element is interactive
                                const interactive = ['BUTTON', 'A', 'INPUT', 'SELECT', 'TEXTAREA'];
                                if (interactive.includes(lower.element.tagName)) {{
                                    anomalies.interaction_conflicts.push({{
                                        clickable_element: lower.selector,
                                        blocking_element: higher.selector,
                                        blocking_z_index: sortedZIndexes[i],
                                        clickable_z_index: sortedZIndexes[j]
                                    }});
                                }}
                            }}
                        }});
                    }});
                }}
            }}
            
            return {{
                viewport: viewport,
                anomalies: anomalies,
                statistics: stats,
                element_count: allElements.length
            }};
        }})();
        "#.to_string()
    }

    fn build_comprehensive_script(
        &self,
        selector: &str,
        proximity: u32,
        index: Option<usize>,
    ) -> String {
        let target_selection = if let Some(idx) = index {
            format!(
                "const elements = document.querySelectorAll('{}');
            if (elements.length <= {}) return {{ error: 'Element at index {} not found' }};
            const target = elements[{}];",
                selector, idx, idx, idx
            )
        } else {
            format!(
                "const target = document.querySelector('{}');
            if (!target) return {{ error: 'Element not found: {}' }};",
                selector, selector
            )
        };

        // Combine key aspects from all focused scripts
        format!(
            r#"
        return (function() {{
            {}
            
            const targetRect = target.getBoundingClientRect();
            const targetStyles = window.getComputedStyle(target);
            
            // Basic element info
            const elementInfo = {{
                selector: '{}',
                tag: target.tagName.toLowerCase(),
                id: target.id || null,
                classes: Array.from(target.classList),
                bounds: {{
                    x: targetRect.x,
                    y: targetRect.y,
                    width: targetRect.width,
                    height: targetRect.height
                }},
                styles: {{
                    display: targetStyles.display,
                    position: targetStyles.position,
                    zIndex: targetStyles.zIndex,
                    opacity: targetStyles.opacity,
                    overflow: targetStyles.overflow,
                    visibility: targetStyles.visibility
                }},
                box_model: {{
                    margin: {{
                        top: parseFloat(targetStyles.marginTop) || 0,
                        right: parseFloat(targetStyles.marginRight) || 0,
                        bottom: parseFloat(targetStyles.marginBottom) || 0,
                        left: parseFloat(targetStyles.marginLeft) || 0
                    }},
                    padding: {{
                        top: parseFloat(targetStyles.paddingTop) || 0,
                        right: parseFloat(targetStyles.paddingRight) || 0,
                        bottom: parseFloat(targetStyles.paddingBottom) || 0,
                        left: parseFloat(targetStyles.paddingLeft) || 0
                    }},
                    border: {{
                        top: parseFloat(targetStyles.borderTopWidth) || 0,
                        right: parseFloat(targetStyles.borderRightWidth) || 0,
                        bottom: parseFloat(targetStyles.borderBottomWidth) || 0,
                        left: parseFloat(targetStyles.borderLeftWidth) || 0
                    }}
                }}
            }};
            
            // Get nearby elements
            const nearbyElements = [];
            const allElements = Array.from(document.querySelectorAll('*'));
            
            allElements.forEach(el => {{
                if (el === target || target.contains(el) || el.contains(target)) return;
                
                const rect = el.getBoundingClientRect();
                const distance = Math.sqrt(
                    Math.pow(rect.x - targetRect.x, 2) + 
                    Math.pow(rect.y - targetRect.y, 2)
                );
                
                if (distance < {}) {{
                    const styles = window.getComputedStyle(el);
                    nearbyElements.push({{
                        selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                        distance: Math.round(distance),
                        bounds: {{
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height
                        }},
                        display: styles.display,
                        position: styles.position,
                        zIndex: styles.zIndex
                    }});
                }}
            }});
            
            // Check children if container
            const children = Array.from(target.children);
            const childrenInfo = children.slice(0, 10).map(child => {{
                const rect = child.getBoundingClientRect();
                return {{
                    tag: child.tagName.toLowerCase(),
                    bounds: {{
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height
                    }}
                }};
            }});
            
            // Viewport info
            const viewport = {{
                width: window.innerWidth,
                height: window.innerHeight,
                scrollWidth: document.documentElement.scrollWidth,
                scrollHeight: document.documentElement.scrollHeight
            }};
            
            return {{
                element: elementInfo,
                nearby_elements: nearbyElements.slice(0, 20),
                children: childrenInfo,
                children_count: children.length,
                viewport: viewport,
                element_count: nearbyElements.length + children.length + 1
            }};
        }})();
        "#,
            target_selection, selector, proximity
        )
    }

    pub async fn close(self) -> Result<()> {
        self.client.close().await?;
        Ok(())
    }
}
