#![allow(clippy::uninlined_format_args)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod browser_manager;
pub mod browser_pool;
mod commands;
mod daemon;
mod errors;
mod profile;
pub mod types;
pub mod webdriver;
mod webdriver_manager;


// Exit codes
const EXIT_SUCCESS: i32 = 0;
const _EXIT_COMMAND_ERROR: i32 = 1;
const _EXIT_ELEMENT_NOT_FOUND: i32 = 2;
const _EXIT_MULTIPLE_ELEMENTS: i32 = 3;
const _EXIT_WEBDRIVER_FAILED: i32 = 4;
const _EXIT_TIMEOUT: i32 = 5;

use crate::commands::daemon::DaemonCommands;
use crate::commands::profile::ProfileCommands;
use crate::commands::session::SessionCommands;
use crate::commands::tab::TabCommands;
use types::{InspectionDepth, OutputFormat};

#[derive(Parser)]
#[command(name = "webprobe")]
#[command(about = "Browser inspection tool for LLMs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Global session name
    #[arg(long, global = true)]
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

    /// Take a screenshot of the page or element
    Screenshot {
        /// URL to navigate to (optional if using tab)
        #[arg(default_value = "")]
        url: String,

        /// CSS selector for element (optional, captures full page if not specified)
        #[arg(short, long)]
        selector: Option<String>,

        /// Output file path (PNG format)
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Viewport size (WIDTHxHEIGHT, e.g., 1920x1080)
        #[arg(long)]
        viewport: Option<String>,

        /// Run browser in visible mode
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Use a persistent tab
        #[arg(long)]
        tab: Option<String>,
    },

    /// Inspect elements within iframes
    Iframe {
        /// URL to navigate to
        url: String,

        /// Iframe selector (e.g., #myframe, iframe[name='content'])
        iframe: String,

        /// Element selector within the iframe
        selector: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,

        /// Use a persistent tab
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

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,

        /// Use a persistent tab (creates if doesn't exist)
        #[arg(long)]
        tab: Option<String>,
    },

    /// Execute JavaScript in the browser
    Eval {
        /// URL to navigate to (optional if using tab)
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

        /// Use a persistent tab (requires daemon)
        #[arg(long)]
        tab: Option<String>,

        /// REQUIRED: Acknowledge security risk of executing arbitrary JavaScript
        #[arg(long)]
        unsafe_eval: bool,
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

    /// Execute multiple commands in batch
    Batch {
        /// Commands as string or file path (use @ prefix for file, e.g., @commands.txt)
        commands: String,

        /// Tab to use (optional, requires daemon)
        #[arg(long)]
        tab: Option<String>,

        /// Browser to use
        #[arg(short, long, default_value = "chrome")]
        browser: String,

        /// Stop on first error
        #[arg(long)]
        stop_on_error: bool,

        /// Run without showing browser window
        #[arg(long, default_value = "true")]
        headless: bool,

        /// Browser profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Viewport size (WIDTHxHEIGHT)
        #[arg(long)]
        viewport: Option<String>,
    },

    /// Detect smart elements (forms, tables, navigation, etc)
    Detect {
        /// URL to inspect
        url: String,

        /// Context selector to limit detection scope
        #[arg(long)]
        context: Option<String>,

        /// Tab to use (requires daemon)
        #[arg(long)]
        tab: Option<String>,

        /// Browser to use
        #[arg(short, long, default_value = "chrome")]
        browser: String,

        /// Browser profile to use
        #[arg(short, long)]
        profile: Option<String>,
    },

    /// Find elements by text content
    FindText {
        /// Text to search for
        text: String,

        /// URL to search (or empty for current tab)
        #[arg(long, default_value = "")]
        url: String,

        /// Element type filter (button, link, input, text, heading, etc)
        #[arg(long, short = 't')]
        element_type: Option<String>,

        /// Use fuzzy matching (partial matches, similar text)
        #[arg(long, short = 'f')]
        fuzzy: bool,

        /// Case sensitive search
        #[arg(long, short = 'c')]
        case_sensitive: bool,

        /// Return all matches (default: all)
        #[arg(long, default_value = "true")]
        all: bool,

        /// Return only Nth match (1-indexed)
        #[arg(long)]
        index: Option<usize>,

        /// Tab to use (for persistent session)
        #[arg(long)]
        tab: Option<String>,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Output format
        #[arg(long, default_value = "json")]
        format: OutputFormat,
    },

    /// Wait for navigation to occur (URL change)
    WaitNavigation {
        /// Current URL or empty to use current tab URL
        #[arg(default_value = "")]
        url: String,

        /// URL pattern to wait for (can be partial match)
        #[arg(long)]
        to: Option<String>,

        /// Maximum time to wait in seconds
        #[arg(long, default_value = "10")]
        timeout: u64,

        /// Tab to use (required)
        #[arg(long)]
        tab: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,
    },

    /// Wait for network to become idle
    WaitIdle {
        /// URL to navigate to (or empty for current tab)
        #[arg(default_value = "")]
        url: String,

        /// Maximum time to wait in milliseconds
        #[arg(long, default_value = "10000")]
        timeout: u64,

        /// Idle time threshold in milliseconds
        #[arg(long, default_value = "500")]
        idle_time: u64,

        /// Tab to use (for persistent session)
        #[arg(long)]
        tab: Option<String>,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Show network log after waiting
        #[arg(long)]
        show_log: bool,
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

    /// Check session status for a tab
    Status {
        /// Tab name to check
        #[arg(long)]
        tab: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,
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


    /// Show version information
    Version,

    /// Check for updates
    Update {
        /// Automatically install if update is available
        #[arg(long)]
        install: bool,
    },

    /// Diagnose layout issues on a webpage
    Diagnose {
        /// URL to diagnose
        url: String,

        /// CSS selector for specific element (optional, analyzes whole page if not specified)
        #[arg(long)]
        selector: Option<String>,

        /// Type of diagnosis (overflow, spacing, alignment, responsiveness, all)
        #[arg(long, default_value = "all")]
        check: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Viewport size for responsive testing
        #[arg(long)]
        viewport: Option<String>,

        /// Run browser in visible mode
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Use a persistent tab
        #[arg(long)]
        tab: Option<String>,
    },

    /// Validate page for accessibility and SEO
    Validate {
        /// URL to validate
        url: String,

        /// Type of validation (accessibility, seo, performance, all)
        #[arg(long, default_value = "all")]
        check: String,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Run browser in visible mode
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,

        /// Use a persistent tab
        #[arg(long)]
        tab: Option<String>,
    },

    /// Compare two pages or states
    Compare {
        /// First URL or state
        url1: String,

        /// Second URL or state
        url2: String,

        /// Type of comparison (visual, structure, content, all)
        #[arg(long, default_value = "all")]
        mode: String,

        /// CSS selector to focus on (optional)
        #[arg(short, long)]
        selector: Option<String>,

        /// Browser to use
        #[arg(short, long, default_value = "firefox")]
        browser: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Viewport size
        #[arg(long)]
        viewport: Option<String>,

        /// Run browser in visible mode
        #[arg(long = "no-headless")]
        no_headless: bool,

        /// Output format
        #[arg(short, long, default_value = "json")]
        format: OutputFormat,

        /// Use a persistent tab
        #[arg(long)]
        tab: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let result = run().await;

    // Always clean up WebDriver processes before exiting
    webdriver_manager::GLOBAL_WEBDRIVER_MANAGER.stop_all();

    // Handle exit codes based on error type
    match result {
        Ok(()) => std::process::exit(EXIT_SUCCESS),
        Err(err) => {
            // Convert to our error type to get proper exit code
            let webprobe_err: errors::WebprobeError = err.into();

            // Output JSON error to stdout for programmatic consumption
            let error_json = json!({
                "error": true,
                "message": webprobe_err.to_string(),
                "exit_code": webprobe_err.exit_code()
            });
            println!(
                "{}",
                serde_json::to_string(&error_json).unwrap_or_else(|_| "{}".to_string())
            );

            // Also log to stderr for human reading
            eprintln!("Error: {}", webprobe_err);
            std::process::exit(webprobe_err.exit_code());
        }
    }
}

async fn run() -> Result<()> {
    // Initialize tracing to stderr (so JSON output to stdout remains clean)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "webprobe=info".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr) // Output logs to stderr
                .with_target(false), // Don't show target module in logs
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Inspect {
            url,
            selector,
            profile,
            format,
            depth,
            all,
            index,
            expect_one,
            viewport,
            tab,
            console,
        } => {
            commands::inspect::handle_inspect(
                url, selector, profile, format, depth, all, index, expect_one, viewport, tab,
                console,
            )
            .await?
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
            commands::r#type::handle_type(
                url,
                selector,
                text,
                clear,
                browser,
                profile,
                no_headless,
                tab,
            )
            .await?
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
            commands::scroll::handle_scroll(
                url,
                selector,
                by_x,
                by_y,
                to,
                browser,
                profile,
                no_headless,
                tab,
            )
            .await?
        }

        Commands::Screenshot {
            url,
            selector,
            output,
            browser,
            profile,
            viewport,
            no_headless,
            tab,
        } => {
            commands::screenshot::handle_screenshot(
                url,
                selector,
                output,
                browser,
                profile,
                viewport,
                no_headless,
                tab,
            )
            .await?
        }

        Commands::Iframe {
            url,
            iframe,
            selector,
            browser,
            profile,
            no_headless,
            format,
            tab,
        } => {
            commands::iframe::handle_iframe(
                url,
                iframe,
                selector,
                browser,
                profile,
                no_headless,
                format,
                tab,
            )
            .await?
        }

        Commands::Layout {
            url,
            selector,
            depth,
            max_elements,
            wait_stable,
            detect_shadow,
            profile,
            format,
            tab,
        } => {
            commands::layout::handle_layout(
                url,
                selector,
                depth,
                max_elements,
                wait_stable,
                detect_shadow,
                profile,
                format,
                tab,
            )
            .await?
        }

        Commands::Eval {
            url,
            code,
            browser,
            profile,
            no_headless,
            format,
            tab,
            unsafe_eval,
        } => {
            commands::eval::handle_eval(
                url,
                code,
                browser,
                profile,
                no_headless,
                format,
                tab,
                unsafe_eval,
            )
            .await?
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
            commands::click::handle_click(
                url,
                selector,
                index,
                browser_name,
                profile,
                viewport,
                no_headless,
                tab,
            )
            .await?
        }

        Commands::Batch {
            commands,
            tab: tab_name,
            browser,
            stop_on_error,
            headless,
            profile,
            viewport,
        } => {
            commands::batch::handle_batch(
                commands,
                tab_name,
                browser,
                stop_on_error,
                headless,
                profile,
                viewport,
            )
            .await?
        }

        Commands::Detect {
            url,
            context,
            tab,
            browser,
            profile,
        } => commands::detect::handle_detect(url, context, tab, browser, profile).await?,

        Commands::FindText {
            text,
            url,
            element_type,
            fuzzy,
            case_sensitive,
            all,
            index,
            tab,
            profile,
            format,
        } => {
            commands::find_text::handle_find_text(
                url,
                text,
                element_type,
                fuzzy,
                case_sensitive,
                all,
                index,
                tab,
                profile,
                format,
            )
            .await?
        }

        Commands::WaitNavigation {
            url,
            to,
            timeout,
            tab: tab_name,
            browser,
        } => {
            commands::wait_navigation::handle_wait_navigation(url, to, timeout, tab_name, browser)
                .await?
        }

        Commands::WaitIdle {
            url,
            timeout,
            idle_time,
            tab,
            browser,
            profile,
            no_headless,
            show_log,
        } => {
            commands::wait_idle::handle_wait_idle(
                url,
                timeout,
                idle_time,
                tab,
                browser,
                profile,
                no_headless,
                show_log,
            )
            .await?
        }

        Commands::Session { command } => commands::session::handle_session(command).await?,

        Commands::Tab { command } => commands::tab::handle_tab(command).await?,

        Commands::Status {
            tab: tab_name,
            browser,
            profile,
        } => commands::status::handle_status(tab_name, browser, profile).await?,

        Commands::Profile { command } => commands::profile::handle_profile(command).await?,

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
            commands::analyze::handle_analyze(
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
            )
            .await?
        }

        Commands::Daemon { command } => commands::daemon::handle_daemon(command).await?,


        Commands::Version => commands::version::handle_version().await?,

        Commands::Diagnose {
            url,
            selector,
            check,
            browser,
            profile,
            viewport,
            no_headless,
            tab,
        } => {
            commands::diagnose::handle_diagnose(
                url,
                selector,
                check,
                browser,
                profile,
                viewport,
                no_headless,
                tab,
            )
            .await?
        }

        Commands::Validate {
            url,
            check,
            browser,
            profile,
            no_headless,
            format,
            tab,
        } => {
            commands::validate::handle_validate(
                url,
                check,
                browser,
                profile,
                no_headless,
                format,
                tab,
            )
            .await?
        }

        Commands::Compare {
            url1,
            url2,
            mode,
            selector,
            browser,
            profile,
            viewport,
            no_headless,
            format,
            tab,
        } => {
            commands::compare::handle_compare(
                url1,
                url2,
                mode,
                selector,
                browser,
                profile,
                viewport,
                no_headless,
                format,
                tab,
            )
            .await?
        }

        Commands::Update { install } => commands::update::handle_update(install).await?,
    }

    Ok(())
}
