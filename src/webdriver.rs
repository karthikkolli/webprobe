use anyhow::{Context, Result};
use fantoccini::{Client, ClientBuilder, Locator};
use serde_json::json;
use tracing::{debug, error, info};

use crate::profile::ProfileManager;
use crate::types::{
    DiagnosticResult, ElementInfo, ElementMetadata, InspectionDepth, LayoutInfo, Position, Size,
    ViewportSize,
};
use crate::webdriver_manager::GLOBAL_WEBDRIVER_MANAGER;

use std::sync::Arc;
use tokio::sync::Mutex;

/// Browser instance for WebDriver automation
#[derive(Debug)]
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

impl std::fmt::Display for BrowserType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrowserType::Firefox => write!(f, "firefox"),
            BrowserType::Chrome => write!(f, "chrome"),
        }
    }
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
    /// Get the default WebDriver URL for this browser type
    /// DEPRECATED: This is only used for detecting external drivers.
    /// The WebDriverManager handles port allocation for managed drivers.
    #[allow(dead_code)]
    pub fn get_default_webdriver_url(&self) -> String {
        match self {
            BrowserType::Firefox => "http://localhost:4444".to_string(),
            BrowserType::Chrome => "http://localhost:9515".to_string(),
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

        // Firefox can take longer to initialize, especially with profiles
        // Try a few times with delays for Firefox
        let mut connect_attempts = if matches!(browser_type, BrowserType::Firefox) {
            3
        } else {
            1
        };

        let client = loop {
            match ClientBuilder::rustls()
                .capabilities(caps.clone())
                .connect(&webdriver_url)
                .await
            {
                Ok(client) => break client,
                Err(e) => {
                    connect_attempts -= 1;
                    if connect_attempts > 0 && matches!(browser_type, BrowserType::Firefox) {
                        info!(
                            "Firefox connection failed, retrying... ({} attempts left)",
                            connect_attempts
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }

                    // All attempts failed, handle the error
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
                        break ClientBuilder::rustls()
                            .capabilities(caps)
                            .connect(&new_url)
                            .await
                            .context("Failed to connect to WebDriver after restart")?;
                    } else {
                        return Err(e).context("Failed to connect to WebDriver");
                    }
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

    pub async fn find_by_text(
        &self,
        url: &str,
        text: &str,
        element_type: Option<&str>,
        fuzzy: bool,
        case_sensitive: bool,
    ) -> Result<Vec<crate::types::TextSearchResult>> {
        if !url.is_empty() {
            self.client.goto(url).await?;
            // Wait a bit for the page to stabilize
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Build JavaScript to find elements by text content
        let script = format!(
            r#"
            return (function() {{
                const searchText = {:?};
                const elementType = {:?};
                const fuzzy = {};
                const caseSensitive = {};
                
                // Normalize text for comparison
                function normalizeText(text) {{
                    return text.trim().replace(/\s+/g, ' ');
                }}
                
                // Check if text matches based on mode
                function textMatches(elementText, searchText) {{
                    const normalizedElement = normalizeText(elementText);
                    const normalizedSearch = normalizeText(searchText);
                    
                    const compareElement = caseSensitive ? normalizedElement : normalizedElement.toLowerCase();
                    const compareSearch = caseSensitive ? normalizedSearch : normalizedSearch.toLowerCase();
                    
                    if (fuzzy) {{
                        // Fuzzy matching - contains, starts with, or similar
                        return compareElement.includes(compareSearch) ||
                               compareElement.startsWith(compareSearch) ||
                               compareElement.endsWith(compareSearch) ||
                               // Levenshtein-like simple similarity
                               (compareSearch.length >= 4 && 
                                compareElement.split(' ').some(word => 
                                    word.includes(compareSearch.substring(0, Math.floor(compareSearch.length * 0.7)))));
                    }} else {{
                        // Exact match
                        return compareElement === compareSearch;
                    }}
                }}
                
                // Get all potential elements
                let selector = '*';
                if (elementType) {{
                    // Support common element type aliases
                    const typeMap = {{
                        'button': 'button, input[type="button"], input[type="submit"], a.button, a.btn, [role="button"]',
                        'link': 'a',
                        'input': 'input, textarea, select',
                        'text': 'input[type="text"], input[type="email"], input[type="password"], input[type="search"], input[type="tel"], input[type="url"], textarea',
                        'heading': 'h1, h2, h3, h4, h5, h6',
                        'image': 'img',
                        'label': 'label',
                        'paragraph': 'p',
                        'div': 'div',
                        'span': 'span',
                        'list': 'ul, ol, dl',
                        'listitem': 'li',
                        'table': 'table',
                        'cell': 'td, th'
                    }};
                    selector = typeMap[elementType.toLowerCase()] || elementType;
                }}
                
                const elements = Array.from(document.querySelectorAll(selector));
                const matches = [];
                
                elements.forEach(el => {{
                    // Get text content (considering various scenarios)
                    let textContent = '';
                    
                    // For inputs, check value and placeholder
                    if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {{
                        textContent = el.value || el.placeholder || '';
                    }}
                    // For buttons and links, get inner text
                    else if (el.tagName === 'BUTTON' || el.tagName === 'A') {{
                        textContent = el.innerText || el.textContent || '';
                    }}
                    // For images, check alt text
                    else if (el.tagName === 'IMG') {{
                        textContent = el.alt || el.title || '';
                    }}
                    // For labels
                    else if (el.tagName === 'LABEL') {{
                        textContent = el.innerText || el.textContent || '';
                    }}
                    // For other elements, get direct text (not from children)
                    else {{
                        // Get only direct text nodes, not from children
                        const walker = document.createTreeWalker(
                            el,
                            NodeFilter.SHOW_TEXT,
                            {{
                                acceptNode: function(node) {{
                                    if (node.parentNode === el) {{
                                        return NodeFilter.FILTER_ACCEPT;
                                    }}
                                    return NodeFilter.FILTER_SKIP;
                                }}
                            }}
                        );
                        let node;
                        while (node = walker.nextNode()) {{
                            textContent += node.textContent + ' ';
                        }}
                        
                        // If no direct text, get all text
                        if (!textContent.trim()) {{
                            textContent = el.innerText || el.textContent || '';
                        }}
                    }}
                    
                    // Also check aria-label and title
                    const ariaLabel = el.getAttribute('aria-label');
                    const title = el.getAttribute('title');
                    
                    // Check if any text matches
                    if (textMatches(textContent, searchText) ||
                        (ariaLabel && textMatches(ariaLabel, searchText)) ||
                        (title && textMatches(title, searchText))) {{
                        
                        const rect = el.getBoundingClientRect();
                        const styles = window.getComputedStyle(el);
                        
                        // Generate a CSS selector for this element
                        let cssSelector = el.tagName.toLowerCase();
                        if (el.id) {{
                            cssSelector = '#' + el.id;
                        }} else if (el.className) {{
                            cssSelector += '.' + el.className.split(' ').filter(c => c).join('.');
                        }}
                        
                        matches.push({{
                            selector: cssSelector,
                            tag: el.tagName.toLowerCase(),
                            text: normalizeText(textContent),
                            position: {{
                                x: Math.round(rect.x),
                                y: Math.round(rect.y)
                            }},
                            size: {{
                                width: Math.round(rect.width),
                                height: Math.round(rect.height)
                            }},
                            visible: rect.width > 0 && rect.height > 0 && 
                                     styles.display !== 'none' && 
                                     styles.visibility !== 'hidden',
                            attributes: {{
                                id: el.id || null,
                                class: el.className || null,
                                href: el.href || null,
                                type: el.type || null,
                                name: el.name || null,
                                'aria-label': ariaLabel,
                                title: title
                            }}
                        }});
                    }}
                }});
                
                return matches;
            }})();
            "#,
            text,
            element_type.unwrap_or(""),
            if fuzzy { "true" } else { "false" },
            if case_sensitive { "true" } else { "false" }
        );

        let value = self.client.execute(&script, vec![]).await?;

        // Convert to TextSearchResult
        let mut result = Vec::new();
        if let Ok(elements) = serde_json::from_value::<Vec<serde_json::Value>>(value) {
            for el in elements {
                if let Ok(info) = serde_json::from_value::<crate::types::TextSearchResult>(el) {
                    result.push(info);
                }
            }
        }

        Ok(result)
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

        // Add a timeout to prevent infinite hangs during navigation
        let navigation_future = self.client.goto(url);
        let timeout_duration = tokio::time::Duration::from_secs(30); // 30 second timeout

        match tokio::time::timeout(timeout_duration, navigation_future).await {
            Ok(Ok(())) => {
                debug!("Navigation to {} completed successfully", url);
            }
            Ok(Err(e)) => {
                error!("Navigation to {} failed: {}", url, e);
                return Err(anyhow::anyhow!("Navigation failed: {}", e));
            }
            Err(_) => {
                error!("Navigation to {} timed out after 30 seconds", url);
                return Err(anyhow::anyhow!("Navigation timeout after 30 seconds"));
            }
        }

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

    /// Find elements including within shadow DOM
    async fn find_elements_with_shadow(
        &self,
        selector: &str,
        include_shadow: bool,
    ) -> Result<Vec<fantoccini::elements::Element>> {
        if !include_shadow {
            // Regular CSS selector search
            return self
                .client
                .find_all(Locator::Css(selector))
                .await
                .context(format!(
                    "Failed to find elements with selector: {}",
                    selector
                ));
        }

        // Use JavaScript to search including shadow DOM
        let script = r#"
            function findInShadowDOM(selector) {
                const results = [];
                
                // First get regular elements
                const regularElements = document.querySelectorAll(selector);
                regularElements.forEach(el => results.push(el));
                
                // Then search in shadow roots
                function searchShadowRoots(root) {
                    const walker = document.createTreeWalker(
                        root,
                        NodeFilter.SHOW_ELEMENT,
                        null,
                        false
                    );
                    
                    let node;
                    while (node = walker.nextNode()) {
                        if (node.shadowRoot) {
                            const shadowElements = node.shadowRoot.querySelectorAll(selector);
                            shadowElements.forEach(el => results.push(el));
                            searchShadowRoots(node.shadowRoot);
                        }
                    }
                }
                
                searchShadowRoots(document);
                return results;
            }
            
            return findInShadowDOM(arguments[0]);
        "#;

        let _elements_js = self
            .client
            .execute(script, vec![json!(selector)])
            .await
            .context("Failed to execute shadow DOM search")?;

        // Convert JavaScript elements to WebDriver elements
        // Note: This is a limitation - we can't directly convert JS elements to WebDriver elements
        // So we'll fall back to regular search for now but log that shadow DOM was searched
        debug!("Searched shadow DOM for selector: {}", selector);

        // For now, return regular search results
        // TODO: Implement proper shadow DOM element handling when fantoccini supports it
        self.client
            .find_all(Locator::Css(selector))
            .await
            .context(format!(
                "Failed to find elements with selector: {}",
                selector
            ))
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
        use std::time::Duration;
        use tokio::time::sleep;

        // Navigate if URL is provided
        if !url.is_empty() {
            debug!("Navigating to: {}", url);
            self.goto(url).await?;
        }

        debug!("Finding elements with selector: {}", selector);

        // Retry logic for finding elements
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;
        const INITIAL_DELAY_MS: u64 = 500;

        // Check if selector indicates shadow DOM search (e.g., starts with ">>>")
        let (use_shadow, actual_selector) = if selector.starts_with(">>>") {
            (true, selector.trim_start_matches(">>>").trim())
        } else {
            (false, selector)
        };

        let elements = loop {
            let found_elements = if use_shadow {
                // Try to find in shadow DOM first
                self.find_elements_with_shadow(actual_selector, true)
                    .await
                    .unwrap_or_default()
            } else {
                self.client
                    .find_all(Locator::Css(actual_selector))
                    .await
                    .unwrap_or_default()
            };

            if !found_elements.is_empty() {
                break found_elements;
            } else if retry_count < MAX_RETRIES {
                retry_count += 1;
                let delay = Duration::from_millis(INITIAL_DELAY_MS * (2_u64.pow(retry_count - 1)));
                debug!(
                    "No elements found, retrying in {:?} (attempt {}/{})",
                    delay, retry_count, MAX_RETRIES
                );
                sleep(delay).await;
            } else {
                return Err(anyhow::anyhow!(
                    "No elements found matching selector '{}' after {} retries{}",
                    actual_selector,
                    MAX_RETRIES,
                    if use_shadow {
                        " (searched shadow DOM)"
                    } else {
                        ""
                    }
                ));
            }
        };

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
            let is_displayed = element.is_displayed().await?;
            let tag_name = element.tag_name().await?;

            // Check if this is a password input field
            let is_password_field = if tag_name.to_lowercase() == "input" {
                element
                    .attr("type")
                    .await
                    .ok()
                    .flatten()
                    .map(|t| t.to_lowercase() == "password")
                    .unwrap_or(false)
            } else {
                false
            };

            // Redact password field values
            let text_content = if is_password_field {
                Some("[REDACTED]".to_string())
            } else {
                element.text().await.ok()
            };

            let computed_styles = json!({
                "display": if is_displayed { "block" } else { "none" },
                "tag": tag_name,
                "index": if all || index.is_some() { Some(i) } else { None },
                "type": if tag_name.to_lowercase() == "input" {
                    element.attr("type").await.ok().flatten()
                } else {
                    None
                },
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

    pub async fn wait_for_navigation(
        &self,
        initial_url: Option<String>,
        target_pattern: Option<&str>,
        timeout_secs: u64,
    ) -> Result<String> {
        use std::time::Duration;
        use tokio::time::sleep;

        // Get initial URL if not provided
        let initial = if let Some(url) = initial_url {
            url
        } else {
            self.client.current_url().await?.to_string()
        };

        debug!("Waiting for navigation from: {}", initial);
        if let Some(pattern) = target_pattern {
            debug!("Target pattern: {}", pattern);
        }

        let start = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(timeout_secs);
        let poll_interval = Duration::from_millis(250);

        while start.elapsed() < timeout_duration {
            sleep(poll_interval).await;

            let current_url = self.client.current_url().await?;
            let current_str = current_url.to_string();

            // Check if URL changed
            if current_str != initial {
                // If no target pattern specified, any change counts
                if target_pattern.is_none() {
                    info!("Navigation detected: {} -> {}", initial, current_str);
                    return Ok(current_str);
                }

                // Check if current URL matches the pattern
                if let Some(pattern) = target_pattern
                    && current_str.contains(pattern)
                {
                    info!("Navigation to target detected: {}", current_str);
                    return Ok(current_str);
                }
            }
        }

        Err(anyhow::anyhow!(
            "Navigation timeout after {} seconds. Still at: {}",
            timeout_secs,
            self.client.current_url().await?
        ))
    }

    pub async fn wait_for_element(
        &self,
        url: &str,
        selector: &str,
        timeout_secs: u64,
        condition: &str,
    ) -> Result<bool> {
        use std::time::Duration;
        use tokio::time::sleep;

        // Navigate if URL is provided
        if !url.is_empty() {
            self.goto(url).await?;
        }

        debug!(
            "Waiting for element: {} with condition: {}",
            selector, condition
        );

        let start = std::time::Instant::now();
        let timeout_duration = Duration::from_secs(timeout_secs);

        // Poll every 500ms
        let poll_interval = Duration::from_millis(500);

        while start.elapsed() < timeout_duration {
            // Try to find the element
            let elements = self.client.find_all(Locator::Css(selector)).await;

            if let Ok(elements) = elements
                && !elements.is_empty()
            {
                let element = &elements[0];

                // Check condition
                let meets_condition = match condition {
                    "present" => true, // Element exists
                    "visible" => element.is_displayed().await.unwrap_or(false),
                    "clickable" => {
                        element.is_displayed().await.unwrap_or(false)
                            && element.is_enabled().await.unwrap_or(false)
                    }
                    _ => {
                        tracing::warn!(
                            "Unknown wait condition: {}, defaulting to 'present'",
                            condition
                        );
                        true
                    }
                };

                if meets_condition {
                    info!(
                        "Element found and meets condition '{}' after {:.2}s",
                        condition,
                        start.elapsed().as_secs_f64()
                    );
                    return Ok(true);
                }
            }

            sleep(poll_interval).await;
        }

        info!("Timeout waiting for element after {} seconds", timeout_secs);
        Ok(false)
    }

    pub async fn get_page_html(&self, url: &str, selector: Option<&str>) -> Result<String> {
        // Navigate if URL is provided
        if !url.is_empty() {
            self.goto(url).await?;
        }

        if let Some(sel) = selector {
            // Get HTML of specific element using querySelector in JavaScript
            let script = format!(
                "var el = document.querySelector('{}'); return el ? el.outerHTML : null;",
                sel.replace('\'', "\\'")
            );

            let html: serde_json::Value = self.client.execute(&script, vec![]).await?;

            match html.as_str() {
                Some(h) => Ok(h.to_string()),
                None => Err(anyhow::anyhow!("Element not found: {}", sel)),
            }
        } else {
            // Get full page HTML
            let html: serde_json::Value = self
                .client
                .execute("return document.documentElement.outerHTML;", vec![])
                .await?;

            Ok(html.as_str().unwrap_or("").to_string())
        }
    }

    pub async fn click_element(
        &self,
        url: &str,
        selector: &str,
        index: Option<usize>,
    ) -> Result<()> {
        use std::time::Duration;
        use tokio::time::sleep;

        // Navigate if URL is provided
        if !url.is_empty() {
            self.goto(url).await?;
        }

        debug!("Finding element with selector: {}", selector);

        // Retry logic for finding elements
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;
        const INITIAL_DELAY_MS: u64 = 500;

        if let Some(idx) = index {
            // Click specific index with retry
            let elements = loop {
                match self.client.find_all(Locator::Css(selector)).await {
                    Ok(elems) if !elems.is_empty() => break elems,
                    Ok(_) | Err(_) if retry_count < MAX_RETRIES => {
                        retry_count += 1;
                        let delay =
                            Duration::from_millis(INITIAL_DELAY_MS * (2_u64.pow(retry_count - 1)));
                        debug!(
                            "No elements found for click, retrying in {:?} (attempt {}/{})",
                            delay, retry_count, MAX_RETRIES
                        );
                        sleep(delay).await;
                    }
                    _ => {
                        return Err(anyhow::anyhow!(
                            "No elements found matching selector '{}' after {} retries",
                            selector,
                            MAX_RETRIES
                        ));
                    }
                }
            };

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
            // Click first element with retry
            let element = loop {
                match self.client.find(Locator::Css(selector)).await {
                    Ok(elem) => break elem,
                    Err(_) if retry_count < MAX_RETRIES => {
                        retry_count += 1;
                        let delay =
                            Duration::from_millis(INITIAL_DELAY_MS * (2_u64.pow(retry_count - 1)));
                        debug!(
                            "Element not found for click, retrying in {:?} (attempt {}/{})",
                            delay, retry_count, MAX_RETRIES
                        );
                        sleep(delay).await;
                    }
                    Err(_) => {
                        return Err(anyhow::anyhow!(
                            "Element not found for selector '{}' after {} retries",
                            selector,
                            MAX_RETRIES
                        ));
                    }
                }
            };

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

        // Check if this is a password field
        let tag_name = element.tag_name().await?;
        if tag_name.to_lowercase() == "input"
            && let Some(input_type) = element.attr("type").await.ok().flatten()
            && input_type.to_lowercase() == "password"
        {
            info!("WARNING: Typing into password field. Value will be redacted in inspect output.");
        }

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

        // Wrap the code to handle both sync and async/Promise results
        let wrapped_code = format!(
            r#"
            return (async function() {{
                try {{
                    const result = {};
                    // Check if result is a Promise
                    if (result && typeof result.then === 'function') {{
                        return await result;
                    }}
                    return result;
                }} catch (error) {{
                    return {{
                        error: true,
                        message: error.message || String(error),
                        stack: error.stack
                    }};
                }}
            }})();
            "#,
            code
        );

        // Execute the wrapped JavaScript
        let result = self
            .client
            .execute(&wrapped_code, vec![])
            .await
            .context("Failed to execute JavaScript")?;

        // Check if the result is an error object
        if let Some(obj) = result.as_object()
            && obj.get("error").and_then(|v| v.as_bool()).unwrap_or(false)
        {
            let msg = obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown JavaScript error");
            let stack = obj.get("stack").and_then(|v| v.as_str()).unwrap_or("");

            return Err(anyhow::anyhow!("JavaScript error: {}\n{}", msg, stack));
        }

        info!("JavaScript execution completed");
        Ok(result)
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

    /// Take a screenshot of the current page
    pub async fn screenshot(&self, path: Option<&str>) -> Result<Vec<u8>> {
        debug!("Taking screenshot");

        // Get the screenshot as bytes
        let screenshot_data = self
            .client
            .screenshot()
            .await
            .context("Failed to take screenshot")?;

        // Save to file if path is provided
        if let Some(file_path) = path {
            use std::fs::File;
            use std::io::Write;

            let mut file = File::create(file_path)
                .context(format!("Failed to create screenshot file: {}", file_path))?;
            file.write_all(&screenshot_data)
                .context(format!("Failed to write screenshot to: {}", file_path))?;

            info!("Screenshot saved to: {}", file_path);
        }

        Ok(screenshot_data)
    }

    /// Take a screenshot of a specific element
    pub async fn screenshot_element(&self, selector: &str, path: Option<&str>) -> Result<Vec<u8>> {
        debug!("Taking screenshot of element: {}", selector);

        // Find the element
        let element = self
            .client
            .find(Locator::Css(selector))
            .await
            .context(format!("Element not found for screenshot: {}", selector))?;

        // Get element screenshot
        let screenshot_data = element.screenshot().await.context(format!(
            "Failed to take screenshot of element: {}",
            selector
        ))?;

        // Save to file if path is provided
        if let Some(file_path) = path {
            use std::fs::File;
            use std::io::Write;

            let mut file = File::create(file_path)
                .context(format!("Failed to create screenshot file: {}", file_path))?;
            file.write_all(&screenshot_data)
                .context(format!("Failed to write screenshot to: {}", file_path))?;

            info!("Element screenshot saved to: {}", file_path);
        }

        Ok(screenshot_data)
    }

    /// Inspect elements within iframes
    pub async fn inspect_iframe(
        &self,
        iframe_selector: &str,
        element_selector: &str,
    ) -> Result<Vec<ElementInfo>> {
        debug!("Switching to iframe: {}", iframe_selector);

        // Use JavaScript to switch to iframe and find elements
        let script = r#"
            // Find the iframe
            const iframe = document.querySelector(arguments[0]);
            if (!iframe) {
                throw new Error('Iframe not found: ' + arguments[0]);
            }
            
            // Get the iframe's document
            const iframeDoc = iframe.contentDocument || iframe.contentWindow.document;
            if (!iframeDoc) {
                throw new Error('Cannot access iframe content (may be cross-origin)');
            }
            
            // Find elements within the iframe
            const elements = iframeDoc.querySelectorAll(arguments[1]);
            const results = [];
            
            elements.forEach(el => {
                const rect = el.getBoundingClientRect();
                const styles = window.getComputedStyle(el);
                
                results.push({
                    tag: el.tagName.toLowerCase(),
                    text: el.textContent || '',
                    position: {
                        x: rect.x + iframe.getBoundingClientRect().x,
                        y: rect.y + iframe.getBoundingClientRect().y
                    },
                    size: {
                        width: rect.width,
                        height: rect.height
                    },
                    display: styles.display,
                    visible: rect.width > 0 && rect.height > 0
                });
            });
            
            return results;
        "#;

        let elements_data = self
            .client
            .execute(
                script,
                vec![json!(iframe_selector), json!(element_selector)],
            )
            .await
            .context("Failed to inspect iframe content")?;

        // Convert JavaScript results to ElementInfo
        let mut results = Vec::new();
        if let serde_json::Value::Array(elements) = elements_data {
            let total_matches = elements.len();
            for (idx, el) in elements.into_iter().enumerate() {
                if let serde_json::Value::Object(obj) = el {
                    let position = Position {
                        x: obj
                            .get("position")
                            .and_then(|p| p.get("x"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        y: obj
                            .get("position")
                            .and_then(|p| p.get("y"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        unit: "px".to_string(),
                    };

                    let size = Size {
                        width: obj
                            .get("size")
                            .and_then(|s| s.get("width"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        height: obj
                            .get("size")
                            .and_then(|s| s.get("height"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        unit: "px".to_string(),
                    };

                    results.push(ElementInfo {
                        selector: element_selector.to_string(),
                        browser: format!("{:?}", self.browser_type),
                        position,
                        size,
                        computed_styles: json!({
                            "display": obj.get("display").and_then(|v| v.as_str()).unwrap_or(""),
                            "tag": obj.get("tag").and_then(|v| v.as_str()).unwrap_or(""),
                        }),
                        text_content: obj
                            .get("text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        children_count: 0,
                        metadata: Some(ElementMetadata {
                            total_matches,
                            returned_index: idx,
                            warning: Some(format!("Element found in iframe: {}", iframe_selector)),
                        }),
                    });
                }
            }
        }

        if results.is_empty() {
            return Err(anyhow::anyhow!(
                "No elements found matching '{}' in iframe '{}'",
                element_selector,
                iframe_selector
            ));
        }

        Ok(results)
    }

    /// Get all iframes on the page
    #[allow(dead_code)]
    pub async fn list_iframes(&self) -> Result<Vec<serde_json::Value>> {
        let script = r#"
            const iframes = Array.from(document.querySelectorAll('iframe'));
            return iframes.map((iframe, index) => {
                const rect = iframe.getBoundingClientRect();
                return {
                    index: index,
                    id: iframe.id || null,
                    name: iframe.name || null,
                    src: iframe.src || null,
                    selector: iframe.id ? `#${iframe.id}` : 
                             iframe.name ? `iframe[name="${iframe.name}"]` :
                             `iframe:nth-of-type(${index + 1})`,
                    width: rect.width,
                    height: rect.height,
                    position: {
                        x: rect.x,
                        y: rect.y
                    },
                    visible: rect.width > 0 && rect.height > 0
                };
            });
        "#;

        let iframes = self
            .client
            .execute(script, vec![])
            .await
            .context("Failed to list iframes")?;

        // Convert to Vec<serde_json::Value>
        if let serde_json::Value::Array(frames) = iframes {
            Ok(frames)
        } else {
            Ok(vec![])
        }
    }

    /// Diagnose layout issues on the page
    pub async fn diagnose_layout(
        &self,
        selector: Option<&str>,
        check_type: &str,
    ) -> Result<serde_json::Value> {
        let script = r#"
            function diagnoseLayout(selector, checkType) {
                const results = {
                    issues: [],
                    warnings: [],
                    suggestions: [],
                    metrics: {}
                };
                
                const elements = selector ? 
                    Array.from(document.querySelectorAll(selector)) : 
                    Array.from(document.querySelectorAll('*'));
                
                // Check for overflow issues
                if (checkType === 'overflow' || checkType === 'all') {
                    // First check if document has horizontal scroll
                    const docScrollWidth = Math.max(
                        document.documentElement.scrollWidth || 0,
                        document.body.scrollWidth || 0,
                        document.documentElement.offsetWidth || 0,
                        document.body.offsetWidth || 0
                    );
                    
                    if (docScrollWidth > window.innerWidth) {
                        results.issues.push({
                            type: 'horizontal_overflow',
                            selector: 'body',
                            width: docScrollWidth,
                            viewport_width: window.innerWidth,
                            overflow: docScrollWidth - window.innerWidth
                        });
                    }
                    
                    elements.forEach(el => {
                        const rect = el.getBoundingClientRect();
                        const styles = window.getComputedStyle(el);
                        
                        // Check multiple width properties
                        const elementWidth = Math.max(
                            el.offsetWidth || 0,
                            el.scrollWidth || 0,
                            parseFloat(styles.width) || 0,
                            rect.width || 0
                        );
                        
                        // If any width measurement exceeds viewport, it's overflow
                        if (elementWidth > window.innerWidth) {
                            const selector = el.tagName.toLowerCase() + 
                                (el.id ? '#' + el.id : '') + 
                                (el.className && typeof el.className === 'string' ? 
                                    '.' + el.className.split(' ').filter(c => c).join('.') : '');
                            
                            // Avoid duplicates
                            const existing = results.issues.find(i => i.selector === selector);
                            if (!existing) {
                                results.issues.push({
                                    type: 'horizontal_overflow',
                                    selector: selector,
                                    width: elementWidth,
                                    viewport_width: window.innerWidth,
                                    overflow: elementWidth - window.innerWidth
                                });
                            }
                        }
                        
                        // Check if element is cut off
                        if (rect.right > window.innerWidth || rect.left < 0) {
                            results.warnings.push({
                                type: 'element_cutoff',
                                selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (el.className ? '.' + el.className.split(' ').filter(c => c).join('.') : ''),
                                position: { left: rect.left, right: rect.right }
                            });
                        }
                        
                        // Check for text overflow
                        if (el.scrollWidth > el.clientWidth || el.scrollHeight > el.clientHeight) {
                            results.warnings.push({
                                type: 'content_overflow',
                                selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                                scrollWidth: el.scrollWidth,
                                clientWidth: el.clientWidth,
                                overflow: styles.overflow
                            });
                        }
                    });
                }
                
                // Check for spacing issues
                if (checkType === 'spacing' || checkType === 'all') {
                    const spacingIssues = [];
                    elements.forEach(el => {
                        const styles = window.getComputedStyle(el);
                        const margin = parseFloat(styles.marginTop) + parseFloat(styles.marginBottom);
                        const padding = parseFloat(styles.paddingTop) + parseFloat(styles.paddingBottom);
                        
                        // Check for excessive spacing
                        if (margin > 100) {
                            spacingIssues.push({
                                type: 'excessive_margin',
                                selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                                margin: margin
                            });
                        }
                        
                        // Check for margin collapse
                        const parent = el.parentElement;
                        if (parent) {
                            const parentStyles = window.getComputedStyle(parent);
                            if (parseFloat(styles.marginTop) > 0 && parseFloat(parentStyles.marginBottom) > 0) {
                                results.warnings.push({
                                    type: 'potential_margin_collapse',
                                    element: el.tagName.toLowerCase(),
                                    parent: parent.tagName.toLowerCase()
                                });
                            }
                        }
                    });
                    if (spacingIssues.length > 0) {
                        results.issues = results.issues.concat(spacingIssues);
                    }
                }
                
                // Check for alignment issues
                if (checkType === 'alignment' || checkType === 'all') {
                    const alignmentMap = new Map();
                    elements.forEach(el => {
                        const rect = el.getBoundingClientRect();
                        const key = Math.round(rect.left);
                        if (!alignmentMap.has(key)) {
                            alignmentMap.set(key, []);
                        }
                        alignmentMap.get(key).push(el);
                    });
                    
                    // Find misaligned elements
                    alignmentMap.forEach((els, xPos) => {
                        if (els.length === 1 && Math.abs(xPos) > 5) {
                            const el = els[0];
                            const siblings = Array.from(el.parentElement?.children || []);
                            if (siblings.length > 1) {
                                results.warnings.push({
                                    type: 'misalignment',
                                    selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                                    position: xPos,
                                    siblings: siblings.length
                                });
                            }
                        }
                    });
                }
                
                // Check for responsiveness issues
                if (checkType === 'responsiveness' || checkType === 'all') {
                    results.metrics.viewport = {
                        width: window.innerWidth,
                        height: window.innerHeight
                    };
                    
                    // Check for fixed widths that might break on mobile
                    elements.forEach(el => {
                        const styles = window.getComputedStyle(el);
                        const width = styles.width;
                        
                        if (width.endsWith('px')) {
                            const widthValue = parseFloat(width);
                            if (widthValue > 400 && !styles.maxWidth) {
                                results.warnings.push({
                                    type: 'fixed_width_no_max',
                                    selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
                                    width: widthValue,
                                    suggestion: 'Consider using max-width or responsive units'
                                });
                            }
                        }
                    });
                    
                    // Check for horizontal scrolling
                    if (document.documentElement.scrollWidth > window.innerWidth) {
                        results.issues.push({
                            type: 'horizontal_scroll',
                            page_width: document.documentElement.scrollWidth,
                            viewport_width: window.innerWidth
                        });
                    }
                }
                
                // Add suggestions based on issues found
                if (results.issues.some(i => i.type === 'horizontal_overflow')) {
                    results.suggestions.push('Use max-width: 100% or overflow-x: auto for wide elements');
                }
                if (results.warnings.some(w => w.type === 'content_overflow')) {
                    results.suggestions.push('Consider using overflow: auto or text-overflow: ellipsis');
                }
                if (results.issues.some(i => i.type === 'excessive_margin')) {
                    results.suggestions.push('Review margin values - consider using consistent spacing scale');
                }
                
                results.summary = {
                    total_issues: results.issues.length,
                    total_warnings: results.warnings.length,
                    check_type: checkType,
                    elements_checked: elements.length
                };
                
                return results;
            }
            
            return diagnoseLayout(arguments[0], arguments[1]);
        "#;

        let selector_arg = selector.map(|s| json!(s)).unwrap_or(json!(null));
        let diagnosis = self
            .client
            .execute(script, vec![selector_arg, json!(check_type)])
            .await
            .context("Failed to diagnose layout")?;

        Ok(diagnosis)
    }

    /// Validate page for accessibility and SEO
    pub async fn validate_page(&self, check_type: &str) -> Result<serde_json::Value> {
        let script = r#"
            function validatePage(checkType) {
                const results = {
                    accessibility: [],
                    seo: [],
                    performance: [],
                    score: 100
                };
                
                // Accessibility checks
                if (checkType === 'accessibility' || checkType === 'all') {
                    // Check for images without alt text
                    document.querySelectorAll('img').forEach(img => {
                        if (!img.alt && !img.getAttribute('aria-label')) {
                            results.accessibility.push({
                                type: 'missing_alt_text',
                                element: 'img',
                                src: img.src,
                                severity: 'error'
                            });
                            results.score -= 5;
                        }
                    });
                    
                    // Check for form inputs without labels
                    document.querySelectorAll('input, select, textarea').forEach(input => {
                        const id = input.id;
                        const label = id ? document.querySelector(`label[for="${id}"]`) : null;
                        const ariaLabel = input.getAttribute('aria-label');
                        
                        if (!label && !ariaLabel && input.type !== 'hidden') {
                            results.accessibility.push({
                                type: 'missing_label',
                                element: input.tagName.toLowerCase(),
                                id: id || 'no-id',
                                severity: 'error'
                            });
                            results.score -= 5;
                        }
                    });
                    
                    // Check for buttons without accessible text
                    document.querySelectorAll('button').forEach(button => {
                        if (!button.textContent.trim() && !button.getAttribute('aria-label')) {
                            results.accessibility.push({
                                type: 'button_no_text',
                                severity: 'error'
                            });
                            results.score -= 3;
                        }
                    });
                    
                    // Check heading hierarchy
                    const headings = Array.from(document.querySelectorAll('h1, h2, h3, h4, h5, h6'));
                    let lastLevel = 0;
                    headings.forEach(h => {
                        const level = parseInt(h.tagName.substring(1));
                        if (level - lastLevel > 1) {
                            results.accessibility.push({
                                type: 'heading_skip',
                                from: 'h' + lastLevel,
                                to: h.tagName.toLowerCase(),
                                severity: 'warning'
                            });
                            results.score -= 2;
                        }
                        lastLevel = level;
                    });
                    
                    // Check for ARIA landmarks
                    const hasMain = document.querySelector('main, [role="main"]');
                    const hasNav = document.querySelector('nav, [role="navigation"]');
                    if (!hasMain) {
                        results.accessibility.push({
                            type: 'missing_main_landmark',
                            severity: 'warning'
                        });
                        results.score -= 2;
                    }
                }
                
                // SEO checks
                if (checkType === 'seo' || checkType === 'all') {
                    // Check for title tag
                    const title = document.querySelector('title');
                    if (!title || !title.textContent.trim()) {
                        results.seo.push({
                            type: 'missing_title',
                            severity: 'error'
                        });
                        results.score -= 10;
                    } else if (title.textContent.length > 60) {
                        results.seo.push({
                            type: 'title_too_long',
                            length: title.textContent.length,
                            severity: 'warning'
                        });
                        results.score -= 2;
                    }
                    
                    // Check for meta description
                    const metaDesc = document.querySelector('meta[name="description"]');
                    if (!metaDesc || !metaDesc.content) {
                        results.seo.push({
                            type: 'missing_meta_description',
                            severity: 'error'
                        });
                        results.score -= 8;
                    } else if (metaDesc.content.length > 160) {
                        results.seo.push({
                            type: 'meta_description_too_long',
                            length: metaDesc.content.length,
                            severity: 'warning'
                        });
                        results.score -= 2;
                    }
                    
                    // Check for h1
                    const h1s = document.querySelectorAll('h1');
                    if (h1s.length === 0) {
                        results.seo.push({
                            type: 'missing_h1',
                            severity: 'error'
                        });
                        results.score -= 5;
                    } else if (h1s.length > 1) {
                        results.seo.push({
                            type: 'multiple_h1',
                            count: h1s.length,
                            severity: 'warning'
                        });
                        results.score -= 3;
                    }
                    
                    // Check for canonical URL
                    const canonical = document.querySelector('link[rel="canonical"]');
                    if (!canonical) {
                        results.seo.push({
                            type: 'missing_canonical',
                            severity: 'warning'
                        });
                        results.score -= 2;
                    }
                }
                
                // Performance checks
                if (checkType === 'performance' || checkType === 'all') {
                    // Count DOM nodes
                    const nodeCount = document.querySelectorAll('*').length;
                    if (nodeCount > 1500) {
                        results.performance.push({
                            type: 'excessive_dom_nodes',
                            count: nodeCount,
                            severity: nodeCount > 3000 ? 'error' : 'warning'
                        });
                        results.score -= nodeCount > 3000 ? 10 : 5;
                    }
                    
                    // Check for large images
                    document.querySelectorAll('img').forEach(img => {
                        if (img.naturalWidth > 2000 || img.naturalHeight > 2000) {
                            results.performance.push({
                                type: 'large_image',
                                src: img.src,
                                dimensions: img.naturalWidth + 'x' + img.naturalHeight,
                                severity: 'warning'
                            });
                            results.score -= 2;
                        }
                    });
                    
                    // Check for inline styles
                    const inlineStyles = document.querySelectorAll('[style]').length;
                    if (inlineStyles > 20) {
                        results.performance.push({
                            type: 'excessive_inline_styles',
                            count: inlineStyles,
                            severity: 'warning'
                        });
                        results.score -= 3;
                    }
                }
                
                results.score = Math.max(0, results.score);
                results.summary = {
                    check_type: checkType,
                    accessibility_issues: results.accessibility.length,
                    seo_issues: results.seo.length,
                    performance_issues: results.performance.length,
                    score: results.score
                };
                
                return results;
            }
            
            return validatePage(arguments[0]);
        "#;

        let validation = self
            .client
            .execute(script, vec![json!(check_type)])
            .await
            .context("Failed to validate page")?;

        Ok(validation)
    }

    /// Compare two pages or states
    pub async fn compare_pages(
        &self,
        url1: &str,
        url2: &str,
        mode: &str,
        selector: Option<&str>,
    ) -> Result<serde_json::Value> {
        // Capture first page
        self.goto(url1).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let script = r#"
            function capturePage(selector) {
                const elements = selector ? 
                    document.querySelectorAll(selector) : 
                    document.querySelectorAll('body *');
                
                const data = {
                    url: window.location.href,
                    title: document.title,
                    elements: [],
                    text_content: [],
                    structure: []
                };
                
                Array.from(elements).forEach(el => {
                    const rect = el.getBoundingClientRect();
                    const styles = window.getComputedStyle(el);
                    
                    data.elements.push({
                        tag: el.tagName.toLowerCase(),
                        id: el.id,
                        classes: Array.from(el.classList),
                        position: { x: rect.x, y: rect.y },
                        size: { width: rect.width, height: rect.height },
                        color: styles.color,
                        background: styles.backgroundColor,
                        display: styles.display,
                        visible: rect.width > 0 && rect.height > 0
                    });
                    
                    if (el.textContent.trim()) {
                        data.text_content.push(el.textContent.trim());
                    }
                    
                    data.structure.push({
                        tag: el.tagName.toLowerCase(),
                        children: el.children.length
                    });
                });
                
                return data;
            }
            
            return capturePage(arguments[0]);
        "#;

        let selector_arg = selector.map(|s| json!(s)).unwrap_or(json!(null));
        let page1_data = self
            .client
            .execute(script, vec![selector_arg.clone()])
            .await?;

        // Capture second page
        self.goto(url2).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let page2_data = self.client.execute(script, vec![selector_arg]).await?;

        // Compare the data
        let comparison_script = r#"
            function comparePagesData(data1, data2, mode) {
                const results = {
                    differences: [],
                    similarities: [],
                    metrics: {}
                };
                
                if (mode === 'visual' || mode === 'all') {
                    // Compare visual properties
                    const maxLen = Math.max(data1.elements.length, data2.elements.length);
                    for (let i = 0; i < maxLen; i++) {
                        const el1 = data1.elements[i];
                        const el2 = data2.elements[i];
                        
                        if (!el1 || !el2) {
                            results.differences.push({
                                type: 'element_count',
                                page1: el1 ? 'exists' : 'missing',
                                page2: el2 ? 'exists' : 'missing'
                            });
                        } else {
                            if (el1.position.x !== el2.position.x || el1.position.y !== el2.position.y) {
                                results.differences.push({
                                    type: 'position',
                                    element: el1.tag + (el1.id ? '#' + el1.id : ''),
                                    page1: el1.position,
                                    page2: el2.position
                                });
                            }
                            if (el1.size.width !== el2.size.width || el1.size.height !== el2.size.height) {
                                results.differences.push({
                                    type: 'size',
                                    element: el1.tag + (el1.id ? '#' + el1.id : ''),
                                    page1: el1.size,
                                    page2: el2.size
                                });
                            }
                            if (el1.color !== el2.color || el1.background !== el2.background) {
                                results.differences.push({
                                    type: 'color',
                                    element: el1.tag + (el1.id ? '#' + el1.id : ''),
                                    page1: { color: el1.color, bg: el1.background },
                                    page2: { color: el2.color, bg: el2.background }
                                });
                            }
                        }
                    }
                }
                
                if (mode === 'content' || mode === 'all') {
                    // Compare text content
                    const text1 = new Set(data1.text_content);
                    const text2 = new Set(data2.text_content);
                    
                    const onlyIn1 = [...text1].filter(t => !text2.has(t));
                    const onlyIn2 = [...text2].filter(t => !text1.has(t));
                    const inBoth = [...text1].filter(t => text2.has(t));
                    
                    if (onlyIn1.length > 0) {
                        results.differences.push({
                            type: 'text_removed',
                            content: onlyIn1
                        });
                    }
                    if (onlyIn2.length > 0) {
                        results.differences.push({
                            type: 'text_added',
                            content: onlyIn2
                        });
                    }
                    
                    results.similarities.push({
                        type: 'shared_content',
                        count: inBoth.length
                    });
                }
                
                if (mode === 'structure' || mode === 'all') {
                    // Compare DOM structure
                    const struct1 = JSON.stringify(data1.structure);
                    const struct2 = JSON.stringify(data2.structure);
                    
                    if (struct1 !== struct2) {
                        results.differences.push({
                            type: 'structure_change',
                            page1_elements: data1.structure.length,
                            page2_elements: data2.structure.length
                        });
                    } else {
                        results.similarities.push({
                            type: 'identical_structure'
                        });
                    }
                }
                
                // Calculate similarity score
                const totalChecks = results.differences.length + results.similarities.length;
                results.metrics.similarity_score = totalChecks > 0 ? 
                    (results.similarities.length / totalChecks) * 100 : 100;
                
                results.metrics.total_differences = results.differences.length;
                results.metrics.comparison_mode = mode;
                
                return results;
            }
            
            return comparePagesData(arguments[0], arguments[1], arguments[2]);
        "#;

        let comparison = self
            .client
            .execute(comparison_script, vec![page1_data, page2_data, json!(mode)])
            .await
            .context("Failed to compare pages")?;

        Ok(comparison)
    }

    /// Get session status information including cookies and localStorage
    pub async fn get_session_status(&self) -> Result<serde_json::Value> {
        // Get current URL
        let current_url = self.client.current_url().await?;

        // Get page title
        let title = self
            .client
            .title()
            .await
            .unwrap_or_else(|_| "N/A".to_string());

        // Get cookies for current domain
        let cookies = self
            .client
            .get_all_cookies()
            .await
            .map(|cookies| cookies.len())
            .unwrap_or(0);

        // Get localStorage keys
        let local_storage_keys = self
            .client
            .execute("return Object.keys(localStorage || {})", vec![])
            .await
            .map(|v| {
                if let Some(arr) = v.as_array() {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            })
            .unwrap_or_else(|_| vec![]);

        // Get sessionStorage keys
        let session_storage_keys = self
            .client
            .execute("return Object.keys(sessionStorage || {})", vec![])
            .await
            .map(|v| {
                if let Some(arr) = v.as_array() {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            })
            .unwrap_or_else(|_| vec![]);

        // Check common auth indicators
        let auth_indicators = json!({
            "has_jwt_in_storage": local_storage_keys.iter().any(|k|
                k.contains("token") || k.contains("jwt") || k.contains("auth")),
            "has_user_in_storage": local_storage_keys.iter().any(|k|
                k.contains("user") || k.contains("profile")),
            "cookies_present": cookies > 0,
        });

        Ok(json!({
            "current_url": current_url.as_str(),
            "title": title,
            "cookies_count": cookies,
            "local_storage_keys": local_storage_keys,
            "session_storage_keys": session_storage_keys,
            "auth_indicators": auth_indicators,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }

    /// Get debug information about the current page state
    pub async fn get_debug_info(&self) -> Result<serde_json::Value> {
        // Get current URL
        let current_url = self.client.current_url().await?;

        // Get page title
        let title = self
            .client
            .title()
            .await
            .unwrap_or_else(|_| "N/A".to_string());

        // Count total elements on page
        let all_elements = self.client.find_all(Locator::Css("*")).await;
        let element_count = all_elements.map(|e| e.len()).unwrap_or(0);

        // Get console errors
        let console_errors = self
            .console_logs
            .lock()
            .await
            .iter()
            .filter(|log| log.level == "error")
            .cloned()
            .collect::<Vec<_>>();

        // Check if page is still loading
        let ready_state = self
            .client
            .execute("return document.readyState", vec![])
            .await
            .map(|v| v.as_str().unwrap_or("unknown").to_string())
            .unwrap_or_else(|_| "error".to_string());

        // Get viewport size
        let viewport = self
            .client
            .execute(
                "return {width: window.innerWidth, height: window.innerHeight}",
                vec![],
            )
            .await
            .ok();

        // Check for common elements
        let has_forms = self
            .client
            .find_all(Locator::Css("form"))
            .await
            .map(|e| !e.is_empty())
            .unwrap_or(false);
        let has_inputs = self
            .client
            .find_all(Locator::Css("input"))
            .await
            .map(|e| !e.is_empty())
            .unwrap_or(false);
        let has_buttons = self
            .client
            .find_all(Locator::Css("button"))
            .await
            .map(|e| !e.is_empty())
            .unwrap_or(false);

        // Get network activity (basic check)
        let network_status = self
            .client
            .execute(
                "return performance.getEntriesByType('resource').length",
                vec![],
            )
            .await
            .ok();

        Ok(json!({
            "current_url": current_url.as_str(),
            "title": title,
            "ready_state": ready_state,
            "element_count": element_count,
            "viewport": viewport,
            "console_errors": console_errors,
            "has_forms": has_forms,
            "has_inputs": has_inputs,
            "has_buttons": has_buttons,
            "network_requests": network_status,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }

    /// Smart element detection - finds forms, tables, navigation, and interactive elements
    pub async fn detect_smart_elements(
        &self,
        url: &str,
        context_selector: Option<&str>,
    ) -> Result<serde_json::Value> {
        // Navigate if URL provided
        if !url.is_empty() {
            self.goto(url).await?;
        }

        let context_clause = context_selector
            .map(|s| format!("document.querySelector('{}') || document", s))
            .unwrap_or_else(|| "document".to_string());

        let script = format!(
            r#"
            return (function() {{
                const context = {};
                if (!context) return {{ error: 'Context not found' }};
                
                // Detect forms and their fields
                const forms = Array.from(context.querySelectorAll('form')).map(form => {{
                    const inputs = Array.from(form.querySelectorAll('input, textarea, select'));
                    return {{
                        id: form.id || null,
                        name: form.name || null,
                        action: form.action || null,
                        method: form.method || 'GET',
                        fields: inputs.map(input => ({{
                            type: input.type || input.tagName.toLowerCase(),
                            name: input.name || null,
                            id: input.id || null,
                            required: input.required || false,
                            placeholder: input.placeholder || null,
                            value: input.type === 'password' ? '[hidden]' : (input.value || null),
                            selector: input.id ? '#' + input.id : 
                                     input.name ? `[name="${{input.name}}"]` :
                                     `${{input.tagName.toLowerCase()}}:nth-of-type(${{Array.from(form.querySelectorAll(input.tagName)).indexOf(input) + 1}})`
                        }})),
                        submit_buttons: Array.from(form.querySelectorAll('button[type="submit"], input[type="submit"]')).map(btn => ({{
                            text: btn.innerText || btn.value || null,
                            selector: btn.id ? '#' + btn.id : 
                                     `button:contains("${{btn.innerText || btn.value}}")`
                        }}))
                    }};
                }});
                
                // Detect tables
                const tables = Array.from(context.querySelectorAll('table')).map(table => {{
                    const headers = Array.from(table.querySelectorAll('th')).map(th => th.innerText.trim());
                    const rows = table.querySelectorAll('tbody tr, tr').length;
                    return {{
                        id: table.id || null,
                        headers: headers,
                        row_count: rows,
                        has_thead: !!table.querySelector('thead'),
                        has_tbody: !!table.querySelector('tbody'),
                        selector: table.id ? '#' + table.id : 
                                 `table:nth-of-type(${{Array.from(context.querySelectorAll('table')).indexOf(table) + 1}})`
                    }};
                }});
                
                // Detect navigation elements
                const navs = Array.from(context.querySelectorAll('nav, [role="navigation"], .nav, .navbar, .menu')).map(nav => {{
                    const links = Array.from(nav.querySelectorAll('a'));
                    return {{
                        tag: nav.tagName.toLowerCase(),
                        class: nav.className || null,
                        id: nav.id || null,
                        link_count: links.length,
                        links: links.slice(0, 10).map(a => ({{
                            text: a.innerText.trim(),
                            href: a.href,
                            selector: a.id ? '#' + a.id : `a:contains("${{a.innerText.trim()}}")`
                        }}))
                    }};
                }});
                
                // Detect interactive elements
                const buttons = Array.from(context.querySelectorAll('button:not([type="submit"])')).map(btn => ({{
                    text: btn.innerText.trim(),
                    onclick: !!btn.onclick,
                    disabled: btn.disabled,
                    selector: btn.id ? '#' + btn.id : 
                             `button:contains("${{btn.innerText.trim()}}")`
                }}));
                
                // Detect clickable elements (with onclick or cursor:pointer)
                const clickables = Array.from(context.querySelectorAll('*')).filter(el => {{
                    const style = window.getComputedStyle(el);
                    return (el.onclick || style.cursor === 'pointer') && 
                           !['A', 'BUTTON', 'INPUT'].includes(el.tagName);
                }}).slice(0, 20).map(el => ({{
                    tag: el.tagName.toLowerCase(),
                    text: el.innerText ? el.innerText.substring(0, 50).trim() : null,
                    id: el.id || null,
                    class: el.className || null,
                    selector: el.id ? '#' + el.id : 
                             el.className ? '.' + el.className.split(' ')[0] :
                             el.tagName.toLowerCase()
                }}));
                
                // Detect modal/dialog elements
                const modals = Array.from(context.querySelectorAll('[role="dialog"], .modal, .dialog, .popup')).map(modal => ({{
                    visible: window.getComputedStyle(modal).display !== 'none',
                    id: modal.id || null,
                    class: modal.className || null,
                    selector: modal.id ? '#' + modal.id : '.' + (modal.className ? modal.className.split(' ')[0] : 'modal')
                }}));
                
                // Detect login/auth forms specifically
                const authForms = forms.filter(form => {{
                    const hasPassword = form.fields.some(f => f.type === 'password');
                    const hasEmail = form.fields.some(f => 
                        f.type === 'email' || 
                        (f.name && f.name.toLowerCase().includes('email')) ||
                        (f.id && f.id.toLowerCase().includes('email'))
                    );
                    const hasUsername = form.fields.some(f => 
                        (f.name && f.name.toLowerCase().includes('user')) ||
                        (f.id && f.id.toLowerCase().includes('user'))
                    );
                    return hasPassword && (hasEmail || hasUsername);
                }});
                
                return {{
                    forms: forms,
                    auth_forms: authForms,
                    tables: tables,
                    navigation: navs,
                    buttons: buttons.slice(0, 20),
                    clickable_elements: clickables,
                    modals: modals,
                    summary: {{
                        form_count: forms.length,
                        has_auth_form: authForms.length > 0,
                        table_count: tables.length,
                        navigation_count: navs.length,
                        button_count: buttons.length,
                        modal_count: modals.length,
                        total_links: context.querySelectorAll('a').length,
                        total_images: context.querySelectorAll('img').length
                    }}
                }};
            }})();
            "#,
            context_clause
        );

        let result = self.client.execute(&script, vec![]).await?;
        Ok(result)
    }

    pub async fn wait_for_network_idle(&self, timeout_ms: u64, idle_time_ms: u64) -> Result<bool> {
        // Inject network monitoring script
        let setup_script = r#"
            (function() {
                if (window.__webprobe_network_monitor) return;
                window.__webprobe_network_monitor = true;
                
                window.__webprobe_pending_requests = 0;
                window.__webprobe_last_activity = Date.now();
                window.__webprobe_network_log = [];
                
                // Monitor fetch requests
                const originalFetch = window.fetch;
                window.fetch = function(...args) {
                    window.__webprobe_pending_requests++;
                    window.__webprobe_last_activity = Date.now();
                    window.__webprobe_network_log.push({
                        type: 'fetch',
                        url: args[0],
                        timestamp: Date.now(),
                        status: 'started'
                    });
                    
                    return originalFetch.apply(this, args)
                        .then(response => {
                            window.__webprobe_pending_requests--;
                            window.__webprobe_last_activity = Date.now();
                            window.__webprobe_network_log.push({
                                type: 'fetch',
                                url: args[0],
                                timestamp: Date.now(),
                                status: 'completed'
                            });
                            return response;
                        })
                        .catch(error => {
                            window.__webprobe_pending_requests--;
                            window.__webprobe_last_activity = Date.now();
                            window.__webprobe_network_log.push({
                                type: 'fetch',
                                url: args[0],
                                timestamp: Date.now(),
                                status: 'failed'
                            });
                            throw error;
                        });
                };
                
                // Monitor XMLHttpRequest
                const XHR = XMLHttpRequest.prototype;
                const originalOpen = XHR.open;
                const originalSend = XHR.send;
                
                XHR.open = function(method, url) {
                    this.__webprobe_url = url;
                    this.__webprobe_method = method;
                    return originalOpen.apply(this, arguments);
                };
                
                XHR.send = function() {
                    window.__webprobe_pending_requests++;
                    window.__webprobe_last_activity = Date.now();
                    
                    const url = this.__webprobe_url;
                    window.__webprobe_network_log.push({
                        type: 'xhr',
                        url: url,
                        timestamp: Date.now(),
                        status: 'started'
                    });
                    
                    this.addEventListener('load', function() {
                        window.__webprobe_pending_requests--;
                        window.__webprobe_last_activity = Date.now();
                        window.__webprobe_network_log.push({
                            type: 'xhr',
                            url: url,
                            timestamp: Date.now(),
                            status: 'completed'
                        });
                    });
                    
                    this.addEventListener('error', function() {
                        window.__webprobe_pending_requests--;
                        window.__webprobe_last_activity = Date.now();
                        window.__webprobe_network_log.push({
                            type: 'xhr',
                            url: url,
                            timestamp: Date.now(),
                            status: 'failed'
                        });
                    });
                    
                    this.addEventListener('abort', function() {
                        window.__webprobe_pending_requests--;
                        window.__webprobe_last_activity = Date.now();
                        window.__webprobe_network_log.push({
                            type: 'xhr',
                            url: url,
                            timestamp: Date.now(),
                            status: 'aborted'
                        });
                    });
                    
                    return originalSend.apply(this, arguments);
                };
                
                // Performance observer for resource timing
                if (window.PerformanceObserver) {
                    const observer = new PerformanceObserver((list) => {
                        for (const entry of list.getEntries()) {
                            if (entry.entryType === 'resource') {
                                window.__webprobe_last_activity = Date.now();
                                window.__webprobe_network_log.push({
                                    type: 'resource',
                                    url: entry.name,
                                    timestamp: Date.now(),
                                    duration: entry.duration,
                                    status: 'loaded'
                                });
                            }
                        }
                    });
                    observer.observe({ entryTypes: ['resource'] });
                }
            })();
            "#
        .to_string();

        // Set up monitoring
        let _ = self.client.execute(&setup_script, vec![]).await;

        // Wait for network to become idle
        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);
        let _idle_threshold = std::time::Duration::from_millis(idle_time_ms);

        loop {
            // Check if timeout exceeded
            if start_time.elapsed() > timeout {
                return Ok(false);
            }

            // Check network status
            let check_script = format!(
                r#"
                return {{
                    pending: window.__webprobe_pending_requests || 0,
                    lastActivity: window.__webprobe_last_activity || 0,
                    currentTime: Date.now(),
                    idleTime: Date.now() - (window.__webprobe_last_activity || Date.now()),
                    recentRequests: (window.__webprobe_network_log || [])
                        .filter(log => Date.now() - log.timestamp < {})
                        .length
                }};
                "#,
                idle_time_ms
            );

            let status = self.client.execute(&check_script, vec![]).await?;

            if let Some(obj) = status.as_object() {
                let pending = obj.get("pending").and_then(|v| v.as_i64()).unwrap_or(0);
                let idle_time = obj.get("idleTime").and_then(|v| v.as_i64()).unwrap_or(0);

                // Network is idle if no pending requests and idle time exceeded
                if pending == 0 && idle_time >= idle_time_ms as i64 {
                    return Ok(true);
                }
            }

            // Wait a bit before checking again
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    pub async fn get_network_log(&self) -> Result<serde_json::Value> {
        let script = r#"
            return {
                pending_requests: window.__webprobe_pending_requests || 0,
                last_activity: window.__webprobe_last_activity || null,
                log: window.__webprobe_network_log || [],
                summary: {
                    total_requests: (window.__webprobe_network_log || []).length,
                    completed: (window.__webprobe_network_log || []).filter(l => l.status === 'completed').length,
                    failed: (window.__webprobe_network_log || []).filter(l => l.status === 'failed').length,
                    pending: window.__webprobe_pending_requests || 0
                }
            };
        "#;

        let result = self.client.execute(script, vec![]).await?;
        Ok(result)
    }

    /// Convert raw analyze results to diagnostic format
    pub fn analyze_to_diagnostic(
        &self,
        focus: &str,
        raw_data: serde_json::Value,
    ) -> DiagnosticResult {
        match focus {
            "spacing" => self.diagnose_spacing(&raw_data),
            "wrapping" => self.diagnose_wrapping(&raw_data),
            "anomalies" => self.diagnose_anomalies(&raw_data),
            _ => self.diagnose_comprehensive(&raw_data),
        }
    }

    fn diagnose_spacing(&self, data: &serde_json::Value) -> DiagnosticResult {
        let mut evidence = Vec::new();
        let mut issues = Vec::new();

        // Check for margin collapse
        if let Some(adjacent) = data.get("adjacent_elements").and_then(|a| a.as_array()) {
            for elem in adjacent {
                if let Some(collapsed) = elem.get("margin_collapsed").and_then(|c| c.as_bool())
                    && collapsed
                    && let Some(pos) = elem.get("position").and_then(|p| p.as_str())
                {
                    evidence.push(format!("Margin collapse detected with {} element", pos));
                    issues.push("margin-collapse");
                }
            }
        }

        // Check for negative margins
        if let Some(target) = data.get("target")
            && let Some(box_model) = target.get("box_model")
            && let Some(margin) = box_model.get("margin")
        {
            for (side, value) in margin.as_object().unwrap_or(&serde_json::Map::new()) {
                if let Some(v) = value.as_f64()
                    && v < 0.0
                {
                    evidence.push(format!("Negative margin-{}: {}px", side, v));
                    issues.push("negative-margin");
                }
            }
        }

        let (diagnosis, confidence, suggested_fix) = if issues.is_empty() {
            ("No spacing issues detected".to_string(), 0.95, None)
        } else if issues.contains(&"margin-collapse") {
            (
                "Margin collapse is affecting element spacing".to_string(),
                0.85,
                Some("Use padding instead of margin, or add border/padding to parent to prevent collapse".to_string())
            )
        } else if issues.contains(&"negative-margin") {
            (
                "Negative margins are being used for positioning".to_string(),
                0.75,
                Some(
                    "Consider using flexbox/grid for layout instead of negative margins"
                        .to_string(),
                ),
            )
        } else {
            (
                "Potential spacing irregularities detected".to_string(),
                0.6,
                None,
            )
        };

        DiagnosticResult {
            diagnosis,
            confidence,
            evidence,
            suggested_fix,
            raw_data: Some(data.clone()),
        }
    }

    fn diagnose_wrapping(&self, data: &serde_json::Value) -> DiagnosticResult {
        let mut evidence = Vec::new();
        let mut overflow_detected = false;

        if let Some(container) = data.get("container")
            && let Some(width) = container.get("available_width").and_then(|w| w.as_f64())
        {
            evidence.push(format!("Container available width: {}px", width));
        }

        if let Some(children) = data.get("children").and_then(|c| c.as_array()) {
            let mut total_width = 0.0;
            let mut wrapped_count = 0;

            for child in children {
                if let Some(wrapped) = child.get("wrapped").and_then(|w| w.as_bool())
                    && wrapped
                {
                    wrapped_count += 1;
                }
                if let Some(width) = child.get("total_width").and_then(|w| w.as_f64()) {
                    total_width += width;
                }
            }

            if wrapped_count > 0 {
                evidence.push(format!("{} elements wrapped to new lines", wrapped_count));
            }

            if let Some(container) = data.get("container")
                && let Some(available) = container.get("available_width").and_then(|w| w.as_f64())
                && total_width > available
            {
                overflow_detected = true;
                evidence.push(format!(
                    "Content width ({:.0}px) exceeds container ({:.0}px)",
                    total_width, available
                ));
            }
        }

        let (diagnosis, confidence, suggested_fix) = if overflow_detected {
            (
                "Content overflow detected - elements are wider than container".to_string(),
                0.9,
                Some(
                    "Reduce element widths, use flexbox with flex-wrap, or adjust container size"
                        .to_string(),
                ),
            )
        } else if evidence.iter().any(|e| e.contains("wrapped")) {
            ("Elements are wrapping as expected".to_string(), 0.85, None)
        } else {
            ("No wrapping issues detected".to_string(), 0.95, None)
        };

        DiagnosticResult {
            diagnosis,
            confidence,
            evidence,
            suggested_fix,
            raw_data: Some(data.clone()),
        }
    }

    fn diagnose_anomalies(&self, data: &serde_json::Value) -> DiagnosticResult {
        let mut evidence = Vec::new();
        let mut critical_issues = Vec::new();

        if let Some(stats) = data.get("statistics") {
            if let Some(hidden) = stats.get("with_display_none").and_then(|v| v.as_u64())
                && hidden > 0
            {
                evidence.push(format!("{} elements have display:none", hidden));
            }
            if let Some(invisible) = stats.get("with_visibility_hidden").and_then(|v| v.as_u64())
                && invisible > 0
            {
                evidence.push(format!("{} elements have visibility:hidden", invisible));
            }
            if let Some(zero) = stats.get("with_zero_dimensions").and_then(|v| v.as_u64())
                && zero > 0
            {
                evidence.push(format!("{} elements have zero width or height", zero));
                critical_issues.push("zero-dimensions");
            }
            if let Some(outside) = stats.get("outside_viewport").and_then(|v| v.as_u64())
                && outside > 0
            {
                evidence.push(format!("{} elements are outside the viewport", outside));
                critical_issues.push("outside-viewport");
            }
        }

        if let Some(anomalies) = data.get("anomalies")
            && let Some(unusual) = anomalies
                .get("elements_with_unusual_properties")
                .and_then(|v| v.as_array())
            && !unusual.is_empty()
        {
            evidence.push(format!(
                "{} elements have unusual properties (negative z-index, etc)",
                unusual.len()
            ));
            critical_issues.push("unusual-properties");
        }

        let (diagnosis, confidence, suggested_fix) = if critical_issues.contains(&"zero-dimensions")
        {
            (
                "Critical: Elements with zero dimensions detected".to_string(),
                0.95,
                Some(
                    "Check for missing content, incorrect CSS, or JavaScript that hasn't run"
                        .to_string(),
                ),
            )
        } else if critical_issues.contains(&"outside-viewport") {
            (
                "Elements are positioned outside the visible viewport".to_string(),
                0.85,
                Some(
                    "Review absolute/fixed positioning and check for negative positions"
                        .to_string(),
                ),
            )
        } else if !evidence.is_empty() {
            (
                "Layout anomalies detected that may affect user experience".to_string(),
                0.75,
                Some("Review element visibility and positioning properties".to_string()),
            )
        } else {
            ("No significant anomalies detected".to_string(), 0.9, None)
        };

        DiagnosticResult {
            diagnosis,
            confidence,
            evidence,
            suggested_fix,
            raw_data: Some(data.clone()),
        }
    }

    fn diagnose_comprehensive(&self, data: &serde_json::Value) -> DiagnosticResult {
        let mut evidence = Vec::new();

        // Gather basic element info
        if let Some(element) = data.get("element") {
            if let Some(bounds) = element.get("bounds")
                && let (Some(w), Some(h)) = (
                    bounds.get("width").and_then(|v| v.as_f64()),
                    bounds.get("height").and_then(|v| v.as_f64()),
                )
            {
                evidence.push(format!("Element size: {:.0}x{:.0}px", w, h));
            }
            if let Some(styles) = element.get("styles") {
                if let Some(display) = styles.get("display").and_then(|v| v.as_str()) {
                    evidence.push(format!("Display: {}", display));
                }
                if let Some(position) = styles.get("position").and_then(|v| v.as_str())
                    && position != "static"
                {
                    evidence.push(format!("Position: {}", position));
                }
            }
        }

        DiagnosticResult {
            diagnosis: "Element analysis complete".to_string(),
            confidence: 0.8,
            evidence,
            suggested_fix: None,
            raw_data: Some(data.clone()),
        }
    }
}
