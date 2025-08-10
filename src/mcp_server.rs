#![allow(unknown_lints)]

use anyhow::Result;
use rmcp::handler::server::tool::Parameters;
use rmcp::{ServerHandler, ServiceExt};
use rmcp_macros::tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{stdin, stdout};
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::{
    types::ViewportSize,
    webdriver::{Browser, BrowserType},
};

/// MCP server for webprobe
pub struct WebProbeMcpServer {
    #[allow(dead_code)] // Used when mcp feature enabled
    tabs: Arc<Mutex<HashMap<String, Browser>>>,
    #[allow(dead_code)] // Used when mcp feature enabled
    default_browser: BrowserType,
    #[allow(dead_code)] // Used when mcp feature enabled
    default_headless: bool,
}

impl Default for WebProbeMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl WebProbeMcpServer {
    pub fn new() -> Self {
        Self {
            tabs: Arc::new(Mutex::new(HashMap::new())),
            default_browser: BrowserType::Firefox,
            default_headless: true,
        }
    }

    #[allow(dead_code)] // Used when mcp feature enabled
    pub async fn run(self) -> Result<()> {
        info!("Starting webprobe MCP server");

        let tools = WebProbeTools::new(self.tabs, self.default_browser, self.default_headless);

        // Create transport from stdio
        let transport = (stdin(), stdout());

        // Start the server
        tools.serve(transport).await?;

        Ok(())
    }
}

/// WebProbe tools for MCP
pub struct WebProbeTools {
    #[allow(dead_code)] // Used by tool methods via macros
    tabs: Arc<Mutex<HashMap<String, Browser>>>,
    #[allow(dead_code)] // Used by tool methods via macros
    default_browser: BrowserType,
    #[allow(dead_code)]
    default_headless: bool,
}

impl WebProbeTools {
    #[allow(dead_code)] // Used by MCP server
    pub fn new(
        tabs: Arc<Mutex<HashMap<String, Browser>>>,
        default_browser: BrowserType,
        default_headless: bool,
    ) -> Self {
        Self {
            tabs,
            default_browser,
            default_headless,
        }
    }

    #[allow(dead_code)] // Used by tool methods
    async fn ensure_tab_exists(
        &self,
        tab: &str,
        browser_type: BrowserType,
        profile: Option<String>,
        viewport: Option<ViewportSize>,
        headless: bool,
    ) -> Result<()> {
        let mut tabs = self.tabs.lock().await;

        if !tabs.contains_key(tab) {
            debug!("Creating new tab: {}", tab);
            let browser = Browser::new(browser_type, profile, viewport, headless).await?;
            tabs.insert(tab.to_string(), browser);
        }

        Ok(())
    }
}

// Implement ServerHandler for WebProbeTools
impl ServerHandler for WebProbeTools {}

// Parameter structs for each tool
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InspectParams {
    pub url: String,
    pub selector: String,
    #[serde(default)]
    pub all: bool,
    pub index: Option<usize>,
    pub tab: Option<String>,
    pub depth: Option<String>,
    pub browser: Option<String>,
    pub profile: Option<String>,
    pub viewport: Option<String>,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

fn default_headless() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ClickParams {
    pub url: String,
    pub selector: String,
    pub tab: Option<String>,
    pub browser: Option<String>,
    pub profile: Option<String>,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TypeTextParams {
    pub url: String,
    pub selector: String,
    pub text: String,
    #[serde(default)]
    pub clear: bool,
    pub tab: Option<String>,
    pub browser: Option<String>,
    pub profile: Option<String>,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScrollParams {
    pub url: String,
    #[serde(default)]
    pub by_x: i32,
    #[serde(default)]
    pub by_y: i32,
    pub to: Option<String>,
    pub tab: Option<String>,
    pub browser: Option<String>,
    pub profile: Option<String>,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EvalParams {
    pub url: String,
    pub script: String,
    pub tab: Option<String>,
    pub browser: Option<String>,
    pub profile: Option<String>,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TabCreateParams {
    pub name: String,
    pub browser: Option<String>,
    pub profile: Option<String>,
    pub viewport: Option<String>,
    #[serde(default = "default_headless")]
    pub headless: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TabCloseParams {
    pub name: String,
}

impl WebProbeTools {
    /// Inspect web elements and get their position, size, and styles
    #[tool(
        description = "Inspect web elements to get position, size, computed styles, and text content. Returns JSON with element details. Use 'tab' parameter to maintain browser state across commands (essential for authentication workflows)."
    )]
    async fn inspect(&self, params: Parameters<InspectParams>) -> Result<serde_json::Value> {
        use crate::types::InspectionDepth;
        let params = params.0;

        debug!(
            "Inspecting {} with selector {}",
            params.url, params.selector
        );

        let browser_type = params.browser.as_deref().map(|b| match b {
            "chrome" => BrowserType::Chrome,
            _ => BrowserType::Firefox,
        });

        let viewport_size = params
            .viewport
            .as_deref()
            .map(ViewportSize::parse)
            .transpose()?;

        let depth = params
            .depth
            .as_deref()
            .map(|d| match d {
                "children" => InspectionDepth::Children,
                "deep" => InspectionDepth::Deep,
                "full" => InspectionDepth::Full,
                _ => InspectionDepth::Shallow,
            })
            .unwrap_or(InspectionDepth::Shallow);

        let result = if let Some(tab_name) = params.tab {
            // Use persistent tab
            let browser_type = browser_type.unwrap_or(self.default_browser);
            let headless = params.headless;

            self.ensure_tab_exists(
                &tab_name,
                browser_type,
                params.profile,
                viewport_size,
                headless,
            )
            .await?;

            let tabs = self.tabs.lock().await;
            let browser = tabs.get(&tab_name).unwrap();
            browser
                .inspect_element(
                    &params.url,
                    &params.selector,
                    depth,
                    params.all,
                    params.index,
                    false,
                )
                .await?
        } else {
            // Create temporary browser for one-shot operation
            let browser_type = browser_type.unwrap_or(self.default_browser);
            let headless = params.headless;
            let browser =
                Browser::new(browser_type, params.profile, viewport_size, headless).await?;
            browser
                .inspect_element(
                    &params.url,
                    &params.selector,
                    depth,
                    params.all,
                    params.index,
                    false,
                )
                .await?
        };

        Ok(json!(result))
    }

    /// Click on a web element
    #[tool(
        description = "Click on a web element. Use 'tab' to maintain state - critical for multi-step workflows like: login forms, navigation, form submissions."
    )]
    async fn click(&self, params: Parameters<ClickParams>) -> Result<String> {
        let params = params.0;
        debug!("Clicking {} on {}", params.selector, params.url);

        let browser_type = params.browser.as_deref().map(|b| match b {
            "chrome" => BrowserType::Chrome,
            _ => BrowserType::Firefox,
        });

        if let Some(tab_name) = params.tab {
            let browser_type = browser_type.unwrap_or(self.default_browser);
            let headless = params.headless;
            self.ensure_tab_exists(&tab_name, browser_type, params.profile, None, headless)
                .await?;
            let tabs = self.tabs.lock().await;
            let browser = tabs.get(&tab_name).unwrap();
            browser
                .click_element(&params.url, &params.selector, None)
                .await?;
        } else {
            let browser_type = browser_type.unwrap_or(self.default_browser);
            let headless = params.headless;
            let browser = Browser::new(browser_type, params.profile, None, headless).await?;
            browser
                .click_element(&params.url, &params.selector, None)
                .await?;
        }

        Ok(format!("Clicked element: {}", params.selector))
    }

    /// Type text into a web element
    #[tool(
        description = "Type text into input fields. Essential for forms, login, search. Use 'clear=true' to replace existing text. Use 'tab' for multi-field forms to maintain state between fields."
    )]
    async fn type_text(&self, params: Parameters<TypeTextParams>) -> Result<String> {
        let params = params.0;
        debug!("Typing into {} on {}", params.selector, params.url);

        let browser_type = params.browser.as_deref().map(|b| match b {
            "chrome" => BrowserType::Chrome,
            _ => BrowserType::Firefox,
        });

        if let Some(tab_name) = params.tab {
            let browser_type = browser_type.unwrap_or(self.default_browser);
            let headless = params.headless;
            self.ensure_tab_exists(&tab_name, browser_type, params.profile, None, headless)
                .await?;
            let tabs = self.tabs.lock().await;
            let browser = tabs.get(&tab_name).unwrap();
            browser
                .type_text(&params.url, &params.selector, &params.text, params.clear)
                .await?;
        } else {
            let browser_type = browser_type.unwrap_or(self.default_browser);
            let headless = params.headless;
            let browser = Browser::new(browser_type, params.profile, None, headless).await?;
            browser
                .type_text(&params.url, &params.selector, &params.text, params.clear)
                .await?;
        }

        Ok(format!("Typed text into element: {}", params.selector))
    }

    /// Create a persistent browser tab
    #[tool(
        description = "Create a persistent browser tab that maintains ALL state (cookies, localStorage, login) across commands. CRITICAL for: authentication workflows, multi-step forms, testing user journeys."
    )]
    async fn tab_create(&self, params: Parameters<TabCreateParams>) -> Result<String> {
        let params = params.0;
        let mut tabs = self.tabs.lock().await;

        if tabs.contains_key(&params.name) {
            return Ok(format!("Tab '{}' already exists", params.name));
        }

        let browser_type = params
            .browser
            .as_deref()
            .map(|b| match b {
                "chrome" => BrowserType::Chrome,
                _ => BrowserType::Firefox,
            })
            .unwrap_or(self.default_browser);

        let viewport_size = params
            .viewport
            .as_deref()
            .map(ViewportSize::parse)
            .transpose()?;

        let headless = params.headless;

        let browser_instance =
            Browser::new(browser_type, params.profile, viewport_size, headless).await?;
        tabs.insert(params.name.clone(), browser_instance);

        info!("Created tab: {}", params.name);
        Ok(format!("Created tab: {}", params.name))
    }

    /// Close a persistent browser tab
    #[tool(description = "Close a persistent browser tab")]
    async fn tab_close(&self, params: Parameters<TabCloseParams>) -> Result<String> {
        let params = params.0;
        let mut tabs = self.tabs.lock().await;

        if tabs.remove(&params.name).is_some() {
            info!("Closed tab: {}", params.name);
            Ok(format!("Closed tab: {}", params.name))
        } else {
            Ok(format!("Tab '{}' not found", params.name))
        }
    }

    /// List all active browser tabs
    #[tool(description = "List all active browser tabs")]
    async fn tab_list(&self) -> Result<serde_json::Value> {
        let tabs = self.tabs.lock().await;
        let tab_names: Vec<String> = tabs.keys().cloned().collect();

        Ok(json!({
            "tabs": tab_names,
            "count": tab_names.len()
        }))
    }
}
