use anyhow::{Context, Result};
use interprocess::local_socket::{
    GenericFilePath, Listener, ListenerOptions, Name, Stream, ToFsName,
    traits::{ListenerExt, Stream as StreamTrait},
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::str::FromStr;
use tracing::{debug, error, info};

use crate::browser_pool::GLOBAL_BROWSER_POOL;
use crate::tab_manager::GLOBAL_TAB_MANAGER;
use crate::types::{ElementInfo, InspectionDepth, ViewportSize};
use crate::webdriver::{BrowserType, ConsoleMessage};

/// Daemon that runs in the background and maintains browser tabs
pub struct Daemon {}

/// Messages that can be sent to the daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonRequest {
    // Tab management
    CreateTab {
        name: String,
        browser_type: String,
        profile: Option<String>,
        viewport: Option<String>,
        headless: bool,
    },
    CloseTab {
        name: String,
    },
    ListTabs {
        profile: Option<String>,
    },

    // Browser operations
    Inspect {
        tab_name: String,
        url: String,
        selector: String,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
        profile: Option<String>,
        browser: BrowserType, // Added browser type
    },
    Type {
        tab_name: String,
        url: String,
        selector: String,
        text: String,
        clear: bool,
        profile: Option<String>,
        browser: BrowserType, // Added browser type
    },
    Click {
        tab_name: String,
        url: String,
        selector: String,
        index: Option<usize>,
        profile: Option<String>,
        browser: BrowserType, // Added browser type
    },
    Scroll {
        tab_name: String,
        url: String,
        selector: Option<String>,
        by_x: i32,
        by_y: i32,
        to: Option<String>,
    },

    // One-shot operations (uses browser pool)
    OneShotInspect {
        url: String,
        selector: String,
        browser: BrowserType,
        depth: InspectionDepth,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
        viewport: Option<ViewportSize>,
        headless: bool,
        console: bool,
    },

    // Daemon control
    Ping,
    Shutdown,
}

/// Responses from the daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonResponse {
    Success(String),
    Error(String),
    TabList(Vec<TabInfo>),
    InspectResult(Vec<ElementInfo>, Option<Vec<ConsoleMessage>>),
    Pong,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TabInfo {
    pub name: String,
    pub url: Option<String>,
    pub profile: Option<String>,
    pub browser_type: String,
}

impl Daemon {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    fn get_socket_path() -> Result<PathBuf> {
        let runtime_dir = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .or_else(|| std::env::temp_dir().into())
            .context("Could not determine runtime directory")?;

        Ok(runtime_dir.join("webprobe-daemon.sock"))
    }

    fn get_socket_name() -> Result<Name<'static>> {
        // Use the same path as get_socket_path() for consistency
        let socket_path = Self::get_socket_path()?;
        let path_string = socket_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Socket path is not valid UTF-8"))?
            .to_owned();
        // Leak the string to get 'static lifetime - this is ok since we only create one daemon
        let path_str: &'static str = Box::leak(path_string.into_boxed_str());
        Ok(path_str.to_fs_name::<GenericFilePath>()?)
    }

    pub fn is_running() -> bool {
        if let Ok(name) = Self::get_socket_name() {
            // Just check if we can connect - don't send data to avoid EOF errors
            Stream::connect(name).is_ok()
        } else {
            false
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        // Check if daemon is already running
        if Self::is_running() {
            anyhow::bail!("Daemon is already running");
        }

        // Remove old socket file if it exists
        let socket_path = Self::get_socket_path()?;
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Create listener
        let name = Self::get_socket_name()?;
        let listener = ListenerOptions::new().name(name).create_sync()?;
        info!("Daemon listening on {:?}", socket_path);

        // Start accepting connections
        self.run_server(listener).await
    }

    async fn run_server(&mut self, listener: Listener) -> Result<()> {
        loop {
            // Use incoming() iterator instead of accept()
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        // Handle client in a separate task
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_client(stream).await {
                                error!("Error handling client: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Error accepting connection: {}", e);
                    }
                }
            }
        }
    }

    async fn handle_client(mut stream: Stream) -> Result<()> {
        // Read request using BufReader for better handling
        let mut reader = BufReader::new(&mut stream);
        let mut request_line = String::new();

        // Read until newline - this handles partial reads better
        let bytes_read = reader.read_line(&mut request_line)?;

        // If we got 0 bytes, it's just a connection check (like from is_running)
        if bytes_read == 0 || request_line.trim().is_empty() {
            // Silent return - not an error, just a connection check
            return Ok(());
        }

        // Remove trailing newline if present
        if request_line.ends_with('\n') {
            request_line.pop();
        }

        let request: DaemonRequest = serde_json::from_str(&request_line)?;

        info!("Received request: {:?}", request);

        // Process request - handle shutdown specially
        if matches!(request, DaemonRequest::Shutdown) {
            info!("Daemon shutting down");

            // Clean up all resources
            let _ = GLOBAL_TAB_MANAGER.close_all_tabs().await;
            let _ = GLOBAL_BROWSER_POOL.close_all().await;

            // Clean up any WebDriver processes started by the daemon
            crate::webdriver_manager::GLOBAL_WEBDRIVER_MANAGER.stop_all();

            // Send success response before shutting down
            let response = DaemonResponse::Success("Daemon shutting down".to_string());
            let response_json = serde_json::to_string(&response)?;
            stream.write_all(response_json.as_bytes())?;
            stream.write_all(b"\n")?;
            stream.flush()?;

            // Clean up socket file
            if let Ok(socket_path) = Self::get_socket_path() {
                let _ = std::fs::remove_file(&socket_path);
            }

            // Give client time to receive the response
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            std::process::exit(0);
        }

        let response = match request {
            DaemonRequest::CreateTab {
                name,
                browser_type,
                profile,
                viewport,
                headless,
            } => {
                match Self::handle_create_tab(name, browser_type, profile, viewport, headless).await
                {
                    Ok(msg) => DaemonResponse::Success(msg),
                    Err(e) => DaemonResponse::Error(e.to_string()),
                }
            }
            DaemonRequest::CloseTab { name } => match GLOBAL_TAB_MANAGER.close_tab(&name).await {
                Ok(true) => DaemonResponse::Success(format!("Tab '{}' closed", name)),
                Ok(false) => DaemonResponse::Error(format!("Tab '{}' not found", name)),
                Err(e) => DaemonResponse::Error(e.to_string()),
            },
            DaemonRequest::ListTabs { profile } => {
                let tabs = if let Some(profile_filter) = profile.as_deref() {
                    GLOBAL_TAB_MANAGER
                        .list_tabs_by_profile(Some(profile_filter))
                        .await
                } else {
                    GLOBAL_TAB_MANAGER.list_tabs().await
                };
                let tab_infos: Vec<TabInfo> = tabs
                    .into_iter()
                    .map(|t| TabInfo {
                        name: t.name,
                        url: t.url,
                        profile: t.profile,
                        browser_type: "unknown".to_string(), // We'd need to track this
                    })
                    .collect();
                DaemonResponse::TabList(tab_infos)
            }
            DaemonRequest::Inspect {
                tab_name,
                url,
                selector,
                all,
                index,
                expect_one,
                profile,
                browser,
            } => {
                match Self::handle_inspect(
                    tab_name, url, selector, all, index, expect_one, profile, browser,
                )
                .await
                {
                    Ok(results) => DaemonResponse::InspectResult(results, None),
                    Err(e) => DaemonResponse::Error(e.to_string()),
                }
            }
            DaemonRequest::Type {
                tab_name,
                url,
                selector,
                text,
                clear,
                profile,
                browser,
            } => {
                match Self::handle_type(tab_name, url, selector, text, clear, profile, browser)
                    .await
                {
                    Ok(msg) => DaemonResponse::Success(msg),
                    Err(e) => DaemonResponse::Error(e.to_string()),
                }
            }
            DaemonRequest::Click {
                tab_name,
                url,
                selector,
                index,
                profile,
                browser,
            } => match Self::handle_click(tab_name, url, selector, index, profile, browser).await {
                Ok(msg) => DaemonResponse::Success(msg),
                Err(e) => DaemonResponse::Error(e.to_string()),
            },
            DaemonRequest::OneShotInspect {
                url,
                selector,
                browser,
                depth,
                all,
                index,
                expect_one,
                viewport,
                headless,
                console,
            } => {
                match Self::handle_oneshot_inspect(
                    url, selector, browser, depth, all, index, expect_one, viewport, headless,
                    console,
                )
                .await
                {
                    Ok((elements, logs)) => DaemonResponse::InspectResult(elements, logs),
                    Err(e) => DaemonResponse::Error(e.to_string()),
                }
            }
            DaemonRequest::Ping => DaemonResponse::Pong,
            DaemonRequest::Shutdown => {
                // Handled above before this match
                unreachable!("Shutdown is handled before this match")
            }
            _ => DaemonResponse::Error("Not implemented".to_string()),
        };

        // Send response
        debug!("Sending response: {:?}", response);
        let response_json = serde_json::to_string(&response)?;
        stream.write_all(response_json.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        debug!("Response sent successfully");

        Ok(())
    }

    async fn handle_create_tab(
        name: String,
        browser_type: String,
        profile: Option<String>,
        viewport: Option<String>,
        headless: bool,
    ) -> Result<String> {
        let browser_type = BrowserType::from_str(&browser_type)?;
        let viewport_size = viewport
            .as_ref()
            .map(|v| ViewportSize::parse(v))
            .transpose()?;

        let _ = GLOBAL_TAB_MANAGER
            .get_or_create_tab(&name, browser_type, profile, viewport_size, headless)
            .await?;

        Ok(format!("Tab '{}' created", name))
    }

    async fn handle_inspect(
        tab_name: String,
        url: String,
        selector: String,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
        profile: Option<String>,
        browser_type: BrowserType, // Added parameter
    ) -> Result<Vec<ElementInfo>> {
        // Get the tab
        let tabs = GLOBAL_TAB_MANAGER.list_tabs().await;
        if !tabs.iter().any(|t| t.name == tab_name) {
            // Create tab with provided browser type
            let _ = GLOBAL_TAB_MANAGER
                .get_or_create_tab(&tab_name, browser_type, profile.clone(), None, true)
                .await?;
        }

        // Get tab and perform inspection
        let tab_lock = GLOBAL_TAB_MANAGER
            .get_or_create_tab(&tab_name, browser_type, profile, None, true)
            .await?;

        let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(&tab_name, &url).await;

        let tab = tab_lock.lock().await;
        if needs_nav {
            tab.browser.goto(&url).await?;
            drop(tab);
            GLOBAL_TAB_MANAGER.update_tab_url(&tab_name, &url).await?;
            let tab = tab_lock.lock().await;
            tab.browser
                .inspect_element(
                    "",
                    &selector,
                    InspectionDepth::Shallow,
                    all,
                    index,
                    expect_one,
                )
                .await
        } else {
            tab.browser
                .inspect_element(
                    "",
                    &selector,
                    InspectionDepth::Shallow,
                    all,
                    index,
                    expect_one,
                )
                .await
        }
    }

    async fn handle_type(
        tab_name: String,
        url: String,
        selector: String,
        text: String,
        clear: bool,
        profile: Option<String>,
        browser_type: BrowserType,
    ) -> Result<String> {
        // Similar to handle_inspect, get or create tab and perform operation
        let tab_lock = GLOBAL_TAB_MANAGER
            .get_or_create_tab(&tab_name, browser_type, profile, None, true)
            .await?;

        let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(&tab_name, &url).await;

        let tab = tab_lock.lock().await;
        if needs_nav {
            tab.browser.goto(&url).await?;
            drop(tab);
            GLOBAL_TAB_MANAGER.update_tab_url(&tab_name, &url).await?;
            let tab = tab_lock.lock().await;
            tab.browser.type_text("", &selector, &text, clear).await?;
        } else {
            tab.browser.type_text("", &selector, &text, clear).await?;
        }

        Ok(format!("Typed text into {}", selector))
    }

    async fn handle_click(
        tab_name: String,
        url: String,
        selector: String,
        index: Option<usize>,
        profile: Option<String>,
        browser_type: BrowserType,
    ) -> Result<String> {
        let tab_lock = GLOBAL_TAB_MANAGER
            .get_or_create_tab(&tab_name, browser_type, profile, None, true)
            .await?;

        let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(&tab_name, &url).await;

        let tab = tab_lock.lock().await;
        if needs_nav {
            tab.browser.goto(&url).await?;
            drop(tab);
            GLOBAL_TAB_MANAGER.update_tab_url(&tab_name, &url).await?;
            let tab = tab_lock.lock().await;
            tab.browser.click_element("", &selector, index).await?;
        } else {
            tab.browser.click_element("", &selector, index).await?;
        }

        Ok(format!("Clicked {}", selector))
    }

    async fn handle_oneshot_inspect(
        url: String,
        selector: String,
        browser: BrowserType,
        depth: InspectionDepth,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
        viewport: Option<ViewportSize>,
        headless: bool,
        console: bool,
    ) -> Result<(Vec<ElementInfo>, Option<Vec<ConsoleMessage>>)> {
        info!(
            "One-shot inspect {} on {} using browser pool",
            selector, url
        );

        // First, clean up any dead browsers in the pool
        if let Err(e) = GLOBAL_BROWSER_POOL.cleanup().await {
            debug!("Pool cleanup warning: {}", e);
        }

        // Get a browser from the pool (will create new if pool is empty or browsers are dead)
        let browser_instance = GLOBAL_BROWSER_POOL.get(browser, viewport, headless).await?;

        // Do the inspection
        let result = if console {
            browser_instance
                .inspect_element_with_console(
                    &url, &selector, depth, all, index, expect_one, console,
                )
                .await
        } else {
            let elements = browser_instance
                .inspect_element(&url, &selector, depth, all, index, expect_one)
                .await?;
            Ok((elements, None))
        };

        // Extract the result before returning browser to pool
        match result {
            Ok((elements, logs)) => {
                // Return browser to pool for reuse
                if let Err(e) = GLOBAL_BROWSER_POOL
                    .return_browser(browser_instance, browser, headless)
                    .await
                {
                    error!("Failed to return browser to pool: {}", e);
                }
                Ok((elements, logs))
            }
            Err(e) => {
                // On error, still try to return the browser to pool
                let _ = GLOBAL_BROWSER_POOL
                    .return_browser(browser_instance, browser, headless)
                    .await;
                Err(e)
            }
        }
    }
}

/// Client for communicating with the daemon
pub struct DaemonClient;

impl DaemonClient {
    pub fn send_request(request: DaemonRequest) -> Result<DaemonResponse> {
        let name = Daemon::get_socket_name()?;

        // Connect to daemon
        let mut stream =
            Stream::connect(name).context("Failed to connect to daemon. Is it running?")?;

        // Send request
        let request_json = serde_json::to_string(&request)?;
        stream.write_all(request_json.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        match reader.read_line(&mut response_line) {
            Ok(0) => {
                // EOF received, daemon closed connection without sending response
                anyhow::bail!("Daemon closed connection without sending response");
            }
            Ok(_) => {
                // Got response
                if response_line.is_empty() {
                    anyhow::bail!("Received empty response from daemon");
                }
                let response: DaemonResponse = serde_json::from_str(&response_line).context(
                    format!("Failed to parse daemon response: {}", response_line),
                )?;
                Ok(response)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn is_daemon_running() -> bool {
        Daemon::is_running()
    }
}
