#![allow(clippy::uninlined_format_args)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::str::FromStr;
use tracing::{debug, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod browser_pool;
mod daemon;
mod profile;
mod tab_manager;
mod types;
mod webdriver;
mod webdriver_manager;

#[cfg(feature = "mcp")]
mod mcp_server;

use profile::ProfileManager;
use tab_manager::GLOBAL_TAB_MANAGER;
use types::{InspectionDepth, OutputFormat, ViewportSize};
use webdriver::{Browser, BrowserType};

#[derive(Parser)]
#[command(name = "webprobe")]
#[command(about = "Browser inspection tool for LLMs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Global session name
    #[arg(short, long, global = true)]
    session: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect an element on a webpage
    Inspect {
        /// URL to inspect
        url: String,

        /// CSS selector for the element
        selector: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use (temporary if not specified)
        #[arg(short, long)]
        profile: Option<String>,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,

        /// Inspection depth
        #[arg(short, long, default_value = "shallow")]
        depth: InspectionDepth,

        /// Return all matching elements instead of just the first
        #[arg(long)]
        all: bool,

        /// Return element at specific index (0-based)
        #[arg(long)]
        index: Option<usize>,

        /// Expect exactly one element (error if multiple found)
        #[arg(long)]
        expect_one: bool,

        /// Set viewport size (WIDTHxHEIGHT, e.g., 1920x1080)
        #[arg(long)]
        viewport: Option<String>,

        /// Run browser in visible mode (disables headless)
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Use a persistent tab (creates if doesn't exist)
        #[arg(long)]
        tab: Option<String>,

        /// Capture and display console logs
        #[arg(long)]
        console: bool,
    },

    /// Type text into an element
    Type {
        /// URL to navigate to
        url: String,

        /// CSS selector for the input element
        selector: String,

        /// Text to type
        text: String,

        /// Clear the field before typing
        #[arg(long, default_value = "false")]
        clear: bool,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use (temporary if not specified)
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode (disables headless)
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Use a persistent tab (creates if doesn't exist)
        #[arg(long)]
        tab: Option<String>,
    },

    /// Scroll the page or an element
    Scroll {
        /// URL to navigate to
        url: String,

        /// CSS selector for element to scroll (optional, defaults to window)
        #[arg(long)]
        selector: Option<String>,

        /// Scroll horizontally by pixels (can be negative)
        #[arg(short = 'x', long, default_value = "0")]
        by_x: i32,

        /// Scroll vertically by pixels (can be negative)
        #[arg(short = 'y', long, default_value = "0")]
        by_y: i32,

        /// Scroll to specific position (format: "x,y" or "top"/"bottom")
        #[arg(long)]
        to: Option<String>,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use (temporary if not specified)
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode (disables headless)
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Use a persistent tab (creates if doesn't exist)
        #[arg(long)]
        tab: Option<String>,
    },

    /// Analyze page with focus on specific context
    Analyze {
        /// URL to analyze
        url: String,

        /// CSS selector for the element of interest
        selector: String,

        /// What context to gather (spacing, overflow, alignment, z-index, all)
        #[arg(long, default_value = "all")]
        focus: String,

        /// Include only elements within this distance (in pixels)
        #[arg(long, default_value = "100")]
        proximity: u32,

        /// Analyze element at specific index (0-based) when multiple match
        #[arg(long)]
        index: Option<usize>,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode (disables headless)
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,

        /// Use a persistent tab (creates if doesn't exist)
        #[arg(long)]
        tab: Option<String>,
    },

    /// Analyze element layout with box model details
    Layout {
        /// URL to analyze
        url: String,

        /// CSS selector for the element
        selector: String,

        /// Maximum depth to traverse (default: 2)
        #[arg(long, default_value = "2")]
        depth: u8,

        /// Maximum number of elements to analyze (safety limit)
        #[arg(long, default_value = "100")]
        max_elements: usize,

        /// Wait for layout to stabilize (in ms)
        #[arg(long, default_value = "500")]
        wait_stable: u64,

        /// Include shadow DOM detection
        #[arg(long, default_value = "false")]
        detect_shadow: bool,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode (disables headless)
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,

        /// Use a persistent tab (creates if doesn't exist)
        #[arg(long)]
        tab: Option<String>,
    },

    /// Execute JavaScript in the browser
    Eval {
        /// URL to navigate to (optional if using session)
        #[arg(long)]
        url: Option<String>,

        /// JavaScript code to execute
        code: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use (temporary if not specified)
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode (disables headless)
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,
    },

    /// Click an element on a webpage
    Click {
        /// URL to navigate to
        url: String,

        /// CSS selector for the element to click
        selector: String,

        /// Click element at specific index (0-based)
        #[arg(long)]
        index: Option<usize>,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use (temporary if not specified)
        #[arg(short, long)]
        profile: Option<String>,

        /// Set viewport size (WIDTHxHEIGHT, e.g., 1920x1080)
        #[arg(long)]
        viewport: Option<String>,

        /// Run browser in visible mode (disables headless)
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Use a persistent tab (creates if doesn't exist)
        #[arg(long)]
        tab: Option<String>,
    },

    /// Manage browser sessions
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },

    /// Manage persistent tabs
    Tab {
        #[command(subcommand)]
        command: TabCommands,
    },

    /// Manage browser profiles
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Manage the background daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },

    /// Run as MCP (Model Context Protocol) server for Claude Code
    #[cfg(feature = "mcp")]
    McpServer,

    /// Show version information
    Version,

    /// Check for updates
    Update {
        /// Automatically install if update is available
        #[arg(long)]
        install: bool,
    },
}

#[derive(Subcommand)]
enum TabCommands {
    /// List all active tabs
    List {
        /// Filter tabs by profile
        #[arg(long)]
        profile: Option<String>,
    },

    /// Close a specific tab
    Close {
        /// Tab name
        name: String,
    },

    /// Close all tabs
    CloseAll,
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Create a new session
    Create {
        /// Session name
        name: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,
    },

    /// Destroy a session
    Destroy {
        /// Session name
        name: String,
    },

    /// List all sessions
    List,
}

#[derive(Subcommand)]
enum ProfileCommands {
    /// Create a new profile
    Create {
        /// Profile name
        name: String,

        /// Browser type
        #[arg(short, long, default_value = "firefox")]
        browser: String,
    },

    /// Delete a profile
    Delete {
        /// Profile name
        name: String,
    },

    /// List all profiles
    List,

    /// Clean up old profiles
    Cleanup {
        /// Delete profiles older than N days
        #[arg(long, default_value = "7")]
        older_than: u32,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Run the daemon (in foreground)
    Run,

    /// Start the daemon (show instructions)
    Start,

    /// Stop the daemon
    Stop,

    /// Check daemon status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let result = run().await;

    // Always clean up WebDriver processes before exiting
    webdriver_manager::GLOBAL_WEBDRIVER_MANAGER.stop_all();

    result
}

async fn run() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "webprobe=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect {
            url,
            selector,
            browser,
            profile,
            format,
            depth,
            all,
            index,
            expect_one,
            viewport,
            no_headless,
            tab,
            console,
        } => {
            info!(
                "Inspecting {} on {} with browser {}",
                selector, url, browser
            );

            // If daemon is running and we have a tab name, use daemon
            if let Some(tab_name) = &tab
                && daemon::DaemonClient::is_daemon_running()
            {
                use daemon::{DaemonClient, DaemonRequest, DaemonResponse};

                let browser_type = BrowserType::from_str(&browser)?;
                let request = DaemonRequest::Inspect {
                    tab_name: tab_name.clone(),
                    url: url.clone(),
                    selector: selector.clone(),
                    all,
                    index,
                    expect_one,
                    profile: profile.clone(),
                    browser: browser_type,
                };

                match DaemonClient::send_request(request) {
                    Ok(DaemonResponse::InspectResult(results, logs)) => {
                        match format {
                            OutputFormat::Json => {
                                if results.len() == 1 && !all {
                                    println!("{}", serde_json::to_string_pretty(&results[0])?);
                                } else {
                                    println!("{}", serde_json::to_string_pretty(&results)?);
                                }
                            }
                            OutputFormat::Simple => {
                                for (i, result) in results.iter().enumerate() {
                                    if results.len() > 1 {
                                        println!(
                                            "[{}] {}: {} element at ({}, {}) {}x{}px",
                                            i,
                                            result.selector,
                                            result
                                                .computed_styles
                                                .get("tag")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("unknown"),
                                            result.position.x,
                                            result.position.y,
                                            result.size.width,
                                            result.size.height
                                        );
                                    } else {
                                        println!(
                                            "{}: {} element at ({}, {}) {}x{}px",
                                            result.selector,
                                            result
                                                .computed_styles
                                                .get("tag")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("unknown"),
                                            result.position.x,
                                            result.position.y,
                                            result.size.width,
                                            result.size.height
                                        );
                                    }
                                    if let Some(text) = &result.text_content {
                                        println!("  Text: {}", text);
                                    }
                                    if result.children_count > 0 {
                                        println!("  Children: {}", result.children_count);
                                    }
                                }
                            }
                        }

                        // Display console logs if captured
                        if let Some(logs) = logs
                            && !logs.is_empty()
                        {
                            eprintln!("\n=== Console Logs ===");
                            for log in logs {
                                eprintln!("[{}] {}: {}", log.timestamp, log.level, log.message);
                            }
                        }

                        return Ok(());
                    }
                    Ok(DaemonResponse::Error(e)) => {
                        eprintln!("Error: {}", e);
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("Failed to communicate with daemon: {}", e);
                        eprintln!("Falling back to direct execution");
                    }
                    _ => {}
                }
            }

            // Fallback to direct execution
            let browser_type = BrowserType::from_str(&browser)?;
            let viewport_size = viewport
                .as_ref()
                .map(|v| ViewportSize::parse(v))
                .transpose()?;

            let (results, console_logs) = if let Some(tab_name) = &tab {
                // Use persistent tab
                let tab_lock = GLOBAL_TAB_MANAGER
                    .get_or_create_tab(tab_name, browser_type, profile, viewport_size, !no_headless)
                    .await?;

                // Check navigation need before locking
                let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(tab_name, &url).await;

                let tab = tab_lock.lock().await;
                let (elements, logs) = if needs_nav {
                    tab.browser.goto(&url).await?;
                    drop(tab);
                    GLOBAL_TAB_MANAGER.update_tab_url(tab_name, &url).await?;
                    let tab = tab_lock.lock().await;
                    tab.browser
                        .inspect_element_with_console(
                            "", &selector, depth, all, index, expect_one, console,
                        )
                        .await?
                } else {
                    tab.browser
                        .inspect_element_with_console(
                            "", &selector, depth, all, index, expect_one, console,
                        )
                        .await?
                };
                (elements, logs)
                // Don't close browser when using tabs
            } else {
                // One-shot mode - try daemon first for better performance
                let (elements, logs) = if daemon::DaemonClient::is_daemon_running() {
                    // Try to use daemon's browser pool
                    match daemon::DaemonClient::send_request(
                        daemon::DaemonRequest::OneShotInspect {
                            url: url.clone(),
                            selector: selector.clone(),
                            browser: browser_type,
                            depth,
                            all,
                            index,
                            expect_one,
                            viewport: viewport_size.clone(),
                            headless: !no_headless,
                            console,
                        },
                    ) {
                        Ok(daemon::DaemonResponse::InspectResult(elements, logs)) => {
                            info!("Used daemon browser pool for faster one-shot");
                            (elements, logs)
                        }
                        Ok(daemon::DaemonResponse::Error(e)) => {
                            // Daemon returned an error, fall back to normal one-shot
                            debug!("Daemon pool failed ({}), using normal one-shot", e);
                            let browser =
                                Browser::new(browser_type, profile, viewport_size, !no_headless)
                                    .await?;
                            let (elements, logs) = browser
                                .inspect_element_with_console(
                                    &url, &selector, depth, all, index, expect_one, console,
                                )
                                .await?;
                            browser.close().await?;
                            (elements, logs)
                        }
                        Ok(_) => {
                            // Unexpected response type
                            info!("Unexpected daemon response type, using normal one-shot");
                            let browser =
                                Browser::new(browser_type, profile, viewport_size, !no_headless)
                                    .await?;
                            let (elements, logs) = browser
                                .inspect_element_with_console(
                                    &url, &selector, depth, all, index, expect_one, console,
                                )
                                .await?;
                            browser.close().await?;
                            (elements, logs)
                        }
                        Err(e) => {
                            // Daemon communication failed, fall back to normal one-shot
                            debug!("Daemon unavailable ({}), using normal one-shot", e);
                            let browser =
                                Browser::new(browser_type, profile, viewport_size, !no_headless)
                                    .await?;
                            let (elements, logs) = browser
                                .inspect_element_with_console(
                                    &url, &selector, depth, all, index, expect_one, console,
                                )
                                .await?;
                            browser.close().await?;
                            (elements, logs)
                        }
                    }
                } else {
                    // No daemon, use normal one-shot
                    let browser =
                        Browser::new(browser_type, profile, viewport_size, !no_headless).await?;
                    let (elements, logs) = browser
                        .inspect_element_with_console(
                            &url, &selector, depth, all, index, expect_one, console,
                        )
                        .await?;
                    browser.close().await?;
                    (elements, logs)
                };
                (elements, logs)
            };

            // Display console logs if captured
            if let Some(logs) = console_logs
                && !logs.is_empty()
            {
                eprintln!("\n=== Console Logs ===");
                for log in &logs {
                    let prefix = match log.level.as_str() {
                        "error" => "❌ ERROR",
                        "warn" => "⚠️  WARN",
                        "info" => "ℹ️  INFO",
                        _ => "📝 LOG",
                    };
                    eprintln!("{}: {}", prefix, log.message);
                }
                eprintln!("==================\n");
            }

            match format {
                OutputFormat::Json => {
                    if results.len() == 1 && !all {
                        // Single element, output as before
                        println!("{}", serde_json::to_string_pretty(&results[0])?);
                    } else {
                        // Multiple elements, output as array
                        println!("{}", serde_json::to_string_pretty(&results)?);
                    }
                }
                OutputFormat::Simple => {
                    for (i, result) in results.iter().enumerate() {
                        if results.len() > 1 {
                            println!(
                                "[{}] {}: {} element at ({}, {}) {}x{}px",
                                i,
                                result.selector,
                                result
                                    .computed_styles
                                    .get("tag")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown"),
                                result.position.x,
                                result.position.y,
                                result.size.width,
                                result.size.height
                            );
                        } else {
                            println!(
                                "{}: {} element at ({}, {}) {}x{}px",
                                result.selector,
                                result
                                    .computed_styles
                                    .get("tag")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown"),
                                result.position.x,
                                result.position.y,
                                result.size.width,
                                result.size.height
                            );
                        }
                        if let Some(text) = &result.text_content {
                            println!("  Text: {}", text);
                        }
                        if result.children_count > 0 {
                            println!("  Children: {}", result.children_count);
                        }
                    }
                }
            }
        }

        Commands::Type {
            url,
            selector,
            text,
            clear,
            browser,
            profile,
            no_headless,
            tab,
        } => {
            info!("Typing into {} on {}", selector, url);

            // If daemon is running and we have a tab name, use daemon
            if let Some(tab_name) = &tab
                && daemon::DaemonClient::is_daemon_running()
            {
                use daemon::{DaemonClient, DaemonRequest, DaemonResponse};

                let browser_type = BrowserType::from_str(&browser)?;
                let request = DaemonRequest::Type {
                    tab_name: tab_name.clone(),
                    url: url.clone(),
                    selector: selector.clone(),
                    text: text.clone(),
                    clear,
                    profile: profile.clone(),
                    browser: browser_type,
                };

                match DaemonClient::send_request(request) {
                    Ok(DaemonResponse::Success(msg)) => {
                        println!("{}", msg);
                        return Ok(());
                    }
                    Ok(DaemonResponse::Error(e)) => {
                        eprintln!("Error: {}", e);
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("Failed to communicate with daemon: {}", e);
                        eprintln!("Falling back to direct execution");
                    }
                    _ => {}
                }
            }

            // Fallback to direct execution
            let browser_type = BrowserType::from_str(&browser)?;

            if let Some(tab_name) = &tab {
                // Use persistent tab
                let tab_lock = GLOBAL_TAB_MANAGER
                    .get_or_create_tab(tab_name, browser_type, profile, None, !no_headless)
                    .await?;

                // Check if we need to navigate before locking the tab
                let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(tab_name, &url).await;

                // Now lock the tab and perform operations
                let tab = tab_lock.lock().await;

                if needs_nav {
                    tab.browser.goto(&url).await?;
                    // Release the lock before calling update_tab_url
                    drop(tab);
                    GLOBAL_TAB_MANAGER.update_tab_url(tab_name, &url).await?;
                    // Re-acquire the lock
                    let tab = tab_lock.lock().await;
                    tab.browser.type_text("", &selector, &text, clear).await?;
                } else {
                    tab.browser.type_text("", &selector, &text, clear).await?;
                }
            } else {
                // One-shot mode
                let browser_obj = Browser::new(browser_type, profile, None, !no_headless).await?;
                browser_obj.type_text(&url, &selector, &text, clear).await?;
                browser_obj.close().await?;
            }

            println!("Successfully typed text into element: {}", selector);
        }

        Commands::Scroll {
            url,
            selector,
            by_x,
            by_y,
            to,
            browser,
            profile,
            no_headless,
            tab,
        } => {
            info!("Scrolling on {}", url);

            let browser_type = BrowserType::from_str(&browser)?;

            if let Some(tab_name) = &tab {
                // Use persistent tab
                let tab_lock = GLOBAL_TAB_MANAGER
                    .get_or_create_tab(tab_name, browser_type, profile, None, !no_headless)
                    .await?;

                let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(tab_name, &url).await;

                let tab = tab_lock.lock().await;
                if needs_nav {
                    tab.browser.goto(&url).await?;
                    drop(tab);
                    GLOBAL_TAB_MANAGER.update_tab_url(tab_name, &url).await?;
                    let tab = tab_lock.lock().await;
                    tab.browser
                        .scroll("", selector.as_deref(), by_x, by_y, to.as_deref())
                        .await?;
                } else {
                    tab.browser
                        .scroll("", selector.as_deref(), by_x, by_y, to.as_deref())
                        .await?;
                }
            } else {
                // One-shot mode
                let browser_obj = Browser::new(browser_type, profile, None, !no_headless).await?;
                browser_obj
                    .scroll(&url, selector.as_deref(), by_x, by_y, to.as_deref())
                    .await?;
                browser_obj.close().await?;
            }

            if let Some(to_pos) = &to {
                println!("Scrolled to position: {}", to_pos);
            } else {
                println!("Scrolled by ({}, {}) pixels", by_x, by_y);
            }
        }

        Commands::Layout {
            url,
            selector,
            depth,
            max_elements,
            wait_stable,
            detect_shadow,
            browser,
            profile,
            no_headless,
            format,
            tab,
        } => {
            info!("Analyzing layout of {} on {}", selector, url);

            let browser_type = BrowserType::from_str(&browser)?;

            let layout = if let Some(tab_name) = &tab {
                // Use persistent tab
                let tab_lock = GLOBAL_TAB_MANAGER
                    .get_or_create_tab(tab_name, browser_type, profile, None, !no_headless)
                    .await?;

                let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(tab_name, &url).await;

                let tab = tab_lock.lock().await;
                if needs_nav {
                    tab.browser.goto(&url).await?;
                    drop(tab);
                    GLOBAL_TAB_MANAGER.update_tab_url(tab_name, &url).await?;
                    let tab = tab_lock.lock().await;
                    tab.browser
                        .analyze_layout(
                            "",
                            &selector,
                            depth,
                            max_elements,
                            wait_stable,
                            detect_shadow,
                        )
                        .await?
                } else {
                    tab.browser
                        .analyze_layout(
                            "",
                            &selector,
                            depth,
                            max_elements,
                            wait_stable,
                            detect_shadow,
                        )
                        .await?
                }
            } else {
                // One-shot mode
                let browser_obj = Browser::new(browser_type, profile, None, !no_headless).await?;
                let result = browser_obj
                    .analyze_layout(
                        &url,
                        &selector,
                        depth,
                        max_elements,
                        wait_stable,
                        detect_shadow,
                    )
                    .await?;
                browser_obj.close().await?;
                result
            };

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&layout)?);
                }
                OutputFormat::Simple => {
                    println!("Layout Analysis for: {}", layout.selector);
                    println!("Tag: {}, Classes: {:?}", layout.tag, layout.classes);
                    println!("Position: ({}, {})", layout.bounds.x, layout.bounds.y);
                    println!("Size: {}x{}", layout.bounds.width, layout.bounds.height);
                    println!("Box Model:");
                    println!(
                        "  Margin: T:{} R:{} B:{} L:{}",
                        layout.box_model.margin.top,
                        layout.box_model.margin.right,
                        layout.box_model.margin.bottom,
                        layout.box_model.margin.left
                    );
                    println!(
                        "  Padding: T:{} R:{} B:{} L:{}",
                        layout.box_model.padding.top,
                        layout.box_model.padding.right,
                        layout.box_model.padding.bottom,
                        layout.box_model.padding.left
                    );
                    println!("Elements analyzed: {}", layout.element_count);
                    if !layout.warnings.is_empty() {
                        println!("Warnings: {:?}", layout.warnings);
                    }
                    if layout.truncated {
                        println!("Note: Analysis was truncated at {} elements", max_elements);
                    }
                }
            }
        }

        Commands::Eval {
            url,
            code,
            browser,
            profile,
            no_headless,
            format,
        } => {
            info!("Executing JavaScript with browser {}", browser);

            let browser_type = BrowserType::from_str(&browser)?;
            let browser = Browser::new(browser_type, profile, None, !no_headless).await?;

            let result = browser.execute_javascript(url.as_deref(), &code).await?;

            browser.close().await?;

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                OutputFormat::Simple => match result {
                    serde_json::Value::String(s) => println!("{}", s),
                    serde_json::Value::Number(n) => println!("{}", n),
                    serde_json::Value::Bool(b) => println!("{}", b),
                    serde_json::Value::Null => println!("null"),
                    _ => println!("{}", result),
                },
            }
        }

        Commands::Click {
            url,
            selector,
            index,
            browser: browser_name,
            profile,
            viewport,
            no_headless,
            tab,
        } => {
            info!("Clicking {} on {}", selector, url);

            // If daemon is running and we have a tab name, use daemon
            if let Some(tab_name) = &tab
                && daemon::DaemonClient::is_daemon_running()
            {
                use daemon::{DaemonClient, DaemonRequest, DaemonResponse};

                let browser_type = BrowserType::from_str(&browser_name)?;
                let request = DaemonRequest::Click {
                    tab_name: tab_name.clone(),
                    url: url.clone(),
                    selector: selector.clone(),
                    index,
                    profile: profile.clone(),
                    browser: browser_type,
                };

                match DaemonClient::send_request(request) {
                    Ok(DaemonResponse::Success(msg)) => {
                        println!("{}", msg);
                        return Ok(());
                    }
                    Ok(DaemonResponse::Error(e)) => {
                        eprintln!("Error: {}", e);
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("Failed to communicate with daemon: {}", e);
                        eprintln!("Falling back to direct execution");
                    }
                    _ => {}
                }
            }

            // Fallback to direct execution
            let browser_type = BrowserType::from_str(&browser_name)?;
            let viewport_size = viewport
                .as_ref()
                .map(|v| ViewportSize::parse(v))
                .transpose()?;

            if let Some(tab_name) = &tab {
                // Use persistent tab
                let tab_lock = GLOBAL_TAB_MANAGER
                    .get_or_create_tab(tab_name, browser_type, profile, viewport_size, !no_headless)
                    .await?;

                // Check navigation need before locking
                let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(tab_name, &url).await;

                let tab = tab_lock.lock().await;
                if needs_nav {
                    tab.browser.goto(&url).await?;
                    drop(tab);
                    GLOBAL_TAB_MANAGER.update_tab_url(tab_name, &url).await?;
                    let tab = tab_lock.lock().await;
                    tab.browser.click_element("", &selector, index).await?;
                } else {
                    tab.browser.click_element("", &selector, index).await?;
                }
            } else {
                // One-shot mode
                let browser =
                    Browser::new(browser_type, profile, viewport_size, !no_headless).await?;
                browser.click_element(&url, &selector, index).await?;
                browser.close().await?;
            }

            if let Some(idx) = index {
                println!(
                    "Successfully clicked element: {} at index {}",
                    selector, idx
                );
            } else {
                println!("Successfully clicked element: {}", selector);
            }
        }

        Commands::Session { command } => match command {
            SessionCommands::Create { name, profile: _ } => {
                info!("Creating session: {}", name);
                println!("Session '{}' created", name);
            }
            SessionCommands::Destroy { name } => {
                info!("Destroying session: {}", name);
                println!("Session '{}' destroyed", name);
            }
            SessionCommands::List => {
                println!("No sessions currently active");
            }
        },

        Commands::Tab { command } => {
            use daemon::{DaemonClient, DaemonRequest, DaemonResponse};

            // Since we're CLI-only, tabs only exist in the daemon
            if !DaemonClient::is_daemon_running() {
                println!("No daemon running. Start with: webprobe daemon run");
                println!("Tabs only persist with the daemon running.");
                return Ok(());
            }

            match command {
                TabCommands::List { profile } => {
                    match DaemonClient::send_request(DaemonRequest::ListTabs {
                        profile: profile.clone(),
                    }) {
                        Ok(DaemonResponse::TabList(tabs)) => {
                            if tabs.is_empty() {
                                println!("No active tabs");
                            } else {
                                println!("Active tabs:");
                                for tab in tabs {
                                    let url_str = tab.url.as_deref().unwrap_or("(no URL)");
                                    let profile_str = tab
                                        .profile
                                        .as_deref()
                                        .map(|p| format!(" [profile: {}]", p))
                                        .unwrap_or_default();
                                    println!("  {}{} - {}", tab.name, profile_str, url_str);
                                }
                            }
                        }
                        Ok(DaemonResponse::Error(e)) => {
                            eprintln!("Error: {}", e);
                        }
                        Err(e) => {
                            eprintln!("Failed to communicate with daemon: {}", e);
                        }
                        _ => {}
                    }
                }
                TabCommands::Close { name } => {
                    match DaemonClient::send_request(DaemonRequest::CloseTab { name: name.clone() })
                    {
                        Ok(DaemonResponse::Success(msg)) => {
                            println!("{}", msg);
                        }
                        Ok(DaemonResponse::Error(e)) => {
                            eprintln!("Error: {}", e);
                        }
                        Err(e) => {
                            eprintln!("Failed to communicate with daemon: {}", e);
                        }
                        _ => {}
                    }
                }
                TabCommands::CloseAll => {
                    // Send close requests for all tabs
                    match DaemonClient::send_request(DaemonRequest::ListTabs { profile: None }) {
                        Ok(DaemonResponse::TabList(tabs)) => {
                            let count = tabs.len();
                            for tab in tabs {
                                let _ = DaemonClient::send_request(DaemonRequest::CloseTab {
                                    name: tab.name,
                                });
                            }
                            println!("Closed {} tab(s)", count);
                        }
                        _ => {
                            eprintln!("Failed to get tab list from daemon");
                        }
                    }
                }
            }
        }

        Commands::Profile { command } => {
            let manager = ProfileManager::new()?;

            match command {
                ProfileCommands::Create { name, browser } => {
                    info!("Creating profile: {} for {}", name, browser);
                    manager.create_profile(&name, &browser)?;
                    println!("Profile '{}' created for {}", name, browser);
                }
                ProfileCommands::Delete { name } => {
                    info!("Deleting profile: {}", name);
                    manager.delete_profile(&name)?;
                    println!("Profile '{}' deleted", name);
                }
                ProfileCommands::List => {
                    let profiles = manager.list_profiles()?;
                    if profiles.is_empty() {
                        println!("No profiles found");
                    } else {
                        println!("Profiles:");
                        for profile in profiles {
                            println!(
                                "  {} ({}) - created: {}, last used: {}{}",
                                profile.name,
                                profile.browser,
                                profile.created_at.format("%Y-%m-%d %H:%M"),
                                profile.last_used.format("%Y-%m-%d %H:%M"),
                                if profile.is_temporary {
                                    " [temporary]"
                                } else {
                                    ""
                                }
                            );
                        }
                    }
                }
                ProfileCommands::Cleanup { older_than } => {
                    info!("Cleaning profiles older than {} days", older_than);
                    let cleaned = manager.cleanup_old_profiles(older_than)?;
                    println!("Cleaned {} profiles", cleaned);
                }
            }
        }

        Commands::Analyze {
            url,
            selector,
            focus,
            proximity,
            index,
            browser,
            profile,
            no_headless,
            format,
            tab,
        } => {
            info!("Analyzing {} on {} with focus: {}", selector, url, focus);

            let browser_type = BrowserType::from_str(&browser)?;

            let analysis = if let Some(tab_name) = &tab {
                // Use persistent tab
                let tab_lock = GLOBAL_TAB_MANAGER
                    .get_or_create_tab(tab_name, browser_type, profile, None, !no_headless)
                    .await?;

                let needs_nav = GLOBAL_TAB_MANAGER.should_navigate(tab_name, &url).await;

                let tab = tab_lock.lock().await;
                if needs_nav {
                    tab.browser.goto(&url).await?;
                    drop(tab);
                    GLOBAL_TAB_MANAGER.update_tab_url(tab_name, &url).await?;
                    let tab = tab_lock.lock().await;
                    tab.browser
                        .analyze_context("", &selector, &focus, proximity, index)
                        .await?
                } else {
                    tab.browser
                        .analyze_context("", &selector, &focus, proximity, index)
                        .await?
                }
            } else {
                // One-shot mode
                let browser_obj = Browser::new(browser_type, profile, None, !no_headless).await?;
                let result = browser_obj
                    .analyze_context(&url, &selector, &focus, proximity, index)
                    .await?;
                browser_obj.close().await?;
                result
            };

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&analysis)?);
                }
                OutputFormat::Simple => {
                    println!("Analysis complete for: {}", selector);
                    println!("Focus: {}", focus);
                    println!(
                        "Elements analyzed: {}",
                        analysis
                            .as_object()
                            .and_then(|o| o.get("element_count"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    );
                }
            }
        }

        Commands::Daemon { command } => {
            use daemon::{Daemon, DaemonClient, DaemonRequest};

            match command {
                DaemonCommands::Run => {
                    if Daemon::is_running() {
                        println!("Daemon is already running");
                    } else {
                        println!("Starting daemon...");
                        let mut daemon = Daemon::new()?;
                        daemon.start().await?;
                    }
                }
                DaemonCommands::Start => {
                    if Daemon::is_running() {
                        println!("Daemon is already running");
                    } else {
                        println!("Starting daemon in background...");

                        // Get log file path
                        let log_dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
                        let log_file = log_dir.join("webprobe-daemon.log");

                        // Fork and daemonize on Unix
                        #[cfg(unix)]
                        {
                            use nix::unistd::{ForkResult, fork, setsid};
                            use std::os::unix::io::AsRawFd;
                            use std::os::unix::process::CommandExt;

                            match unsafe { fork() } {
                                Ok(ForkResult::Parent { .. }) => {
                                    // Parent process: wait a bit and check if daemon started
                                    std::thread::sleep(std::time::Duration::from_millis(500));

                                    if Daemon::is_running() {
                                        println!("Daemon started successfully");
                                        println!("Log file: {}", log_file.display());
                                    } else {
                                        eprintln!(
                                            "Failed to start daemon. Check log file: {}",
                                            log_file.display()
                                        );
                                    }
                                }
                                Ok(ForkResult::Child) => {
                                    // Child process: become a daemon
                                    // Create new session
                                    let _ = setsid();

                                    // Redirect stdout/stderr to log file
                                    let log_fd = std::fs::OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(&log_file)?;

                                    // Redirect stdout and stderr
                                    let log_fd = log_fd.as_raw_fd();
                                    nix::unistd::dup2(log_fd, 1)?; // stdout
                                    nix::unistd::dup2(log_fd, 2)?; // stderr

                                    // Close stdin
                                    nix::unistd::close(0)?;

                                    // Execute ourselves with daemon run
                                    // This creates a fresh process without Tokio runtime issues
                                    let exe_path = std::env::current_exe()?;
                                    let _ = std::process::Command::new(exe_path)
                                        .arg("daemon")
                                        .arg("run")
                                        .exec();

                                    // If exec fails, exit
                                    std::process::exit(1);
                                }
                                Err(e) => {
                                    eprintln!("Fork failed: {}", e);
                                }
                            }
                        }

                        #[cfg(not(unix))]
                        {
                            // Windows or other platforms: use simple spawn approach
                            use std::process::Command;
                            let exe_path = std::env::current_exe()?;

                            let child = Command::new(&exe_path)
                                .arg("daemon")
                                .arg("run")
                                .stdin(std::process::Stdio::null())
                                .stdout(std::fs::File::create(&log_file)?)
                                .stderr(std::fs::File::create(&log_file)?)
                                .spawn()?;

                            std::mem::forget(child);

                            std::thread::sleep(std::time::Duration::from_millis(500));

                            if Daemon::is_running() {
                                println!("Daemon started successfully");
                                println!("Log file: {}", log_file.display());
                            } else {
                                eprintln!(
                                    "Failed to start daemon. Check log file: {}",
                                    log_file.display()
                                );
                            }
                        }
                    }
                }
                DaemonCommands::Stop => {
                    if DaemonClient::is_daemon_running() {
                        match DaemonClient::send_request(DaemonRequest::Shutdown) {
                            Ok(_) => println!("Daemon stopped"),
                            Err(e) => println!("Failed to stop daemon: {}", e),
                        }
                    } else {
                        println!("Daemon is not running");
                    }
                }
                DaemonCommands::Status => {
                    if DaemonClient::is_daemon_running() {
                        match DaemonClient::send_request(DaemonRequest::Ping) {
                            Ok(daemon::DaemonResponse::Pong) => {
                                println!("Daemon is running");

                                // List tabs
                                if let Ok(daemon::DaemonResponse::TabList(tabs)) =
                                    DaemonClient::send_request(DaemonRequest::ListTabs {
                                        profile: None,
                                    })
                                    && !tabs.is_empty()
                                {
                                    println!("\nActive tabs:");
                                    for tab in tabs {
                                        println!(
                                            "  {} - {}",
                                            tab.name,
                                            tab.url.as_deref().unwrap_or("(no URL)")
                                        );
                                    }
                                }
                            }
                            _ => println!("Daemon is not responding properly"),
                        }
                    } else {
                        println!("Daemon is not running");
                    }
                }
            }
        }

        #[cfg(feature = "mcp")]
        Commands::McpServer => {
            info!("Starting MCP server for Claude Code");

            // Import MCP server module
            use webprobe::mcp_server::WebProbeMcpServer;

            // Create and run the MCP server
            let server = WebProbeMcpServer::new();
            server.run().await?;
        }

        Commands::Version => {
            const VERSION: &str = env!("CARGO_PKG_VERSION");
            const NAME: &str = env!("CARGO_PKG_NAME");
            const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

            println!("{} v{}", NAME, VERSION);
            println!("By: {}", AUTHORS);
            println!("Repository: https://github.com/karthikkolli/webprobe");

            // Check if running from a package manager
            if std::env::var("HOMEBREW_PREFIX").is_ok() {
                println!("Installed via: Homebrew");
            } else if std::path::Path::new("/usr/bin/apt").exists()
                && std::path::Path::new("/usr/share/doc/webprobe").exists()
            {
                println!("Installed via: APT (.deb)");
            } else if std::path::Path::new("/usr/bin/dnf").exists()
                || std::path::Path::new("/usr/bin/yum").exists()
            {
                println!("Installed via: RPM");
            }
        }

        Commands::Update { install } => {
            check_for_updates(install).await?;
        }
    }

    Ok(())
}

async fn check_for_updates(auto_install: bool) -> Result<()> {
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    const REPO: &str = "karthikkolli/webprobe";

    println!("Current version: v{}", CURRENT_VERSION);
    println!("Checking for updates...");

    // Fetch latest release from GitHub API
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);

    let response = client
        .get(&url)
        .header("User-Agent", "webprobe-updater")
        .send()
        .await?;

    if !response.status().is_success() {
        println!("Failed to check for updates: {}", response.status());
        return Ok(());
    }

    let release: serde_json::Value = response.json().await?;
    let latest_version = release["tag_name"]
        .as_str()
        .unwrap_or("unknown")
        .trim_start_matches('v');

    if latest_version == CURRENT_VERSION {
        println!("✅ You are running the latest version!");
        return Ok(());
    }

    println!("🆕 New version available: v{}", latest_version);
    println!(
        "Release notes: {}",
        release["html_url"].as_str().unwrap_or("")
    );

    // Detect installation method
    let update_cmd = if std::env::var("HOMEBREW_PREFIX").is_ok() {
        Some("brew upgrade webprobe")
    } else if std::path::Path::new("/usr/bin/apt").exists()
        && std::path::Path::new("/usr/share/doc/webprobe").exists()
    {
        Some("sudo apt update && sudo apt upgrade webprobe")
    } else if std::path::Path::new("/usr/bin/dnf").exists() {
        Some("sudo dnf upgrade webprobe")
    } else if std::path::Path::new("/usr/bin/yum").exists() {
        Some("sudo yum update webprobe")
    } else if std::path::Path::new("/usr/local/bin/webprobe").exists() {
        Some(
            "curl -fsSL https://raw.githubusercontent.com/karthikkolli/webprobe/main/install.sh | bash",
        )
    } else {
        None
    };

    if let Some(cmd) = update_cmd {
        println!("\nTo update, run:");
        println!("  {}", cmd);

        if auto_install {
            println!("\nAttempting automatic update...");
            let shell = if cfg!(target_os = "windows") {
                "cmd"
            } else {
                "sh"
            };
            let flag = if cfg!(target_os = "windows") {
                "/C"
            } else {
                "-c"
            };

            let status = std::process::Command::new(shell)
                .arg(flag)
                .arg(cmd)
                .status()?;

            if status.success() {
                println!("✅ Update completed successfully!");
                println!("Please restart webprobe to use the new version.");
            } else {
                println!("❌ Automatic update failed. Please run the update command manually.");
            }
        }
    } else {
        println!("\nTo update manually:");
        println!(
            "  1. Download from: https://github.com/{}/releases/latest",
            REPO
        );
        println!("  2. Or reinstall using: cargo install webprobe");
    }

    Ok(())
}
