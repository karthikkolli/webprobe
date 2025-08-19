//! # webprobe
#![allow(clippy::uninlined_format_args)]
//!
//! CLI tool for programmatic web inspection, designed for LLMs and automation.
//!
//! Provides precise element measurements, positions, and computed styles without visual rendering.
//!
//! ## Primary Use Case
//!
//! This crate is primarily designed as a CLI tool for LLMs to inspect web pages.
//! While it can be used as a library, the main interface is through the command line.
//!
//! ## Installation
//!
//! ```bash
//! cargo install webprobe
//! ```
//!
//! ## CLI Usage
//!
//! ### Basic Commands
//!
//! ```bash
//! # Inspect an element (returns position, size, styles)
//! webprobe inspect "https://example.com" "h1"
//!
//! # Inspect all matching elements
//! webprobe inspect "https://example.com" ".card" --all
//!
//! # Inspect specific element by index
//! webprobe inspect "https://example.com" ".nav-item" --index 2
//!
//! # Click an element
//! webprobe click "https://example.com" "button.submit"
//!
//! # Type text into an input
//! webprobe type "https://example.com" "input[name='search']" "query text"
//!
//! # Type with clearing field first
//! webprobe type "https://example.com" "input#email" "new@email.com" --clear
//!
//! # Scroll the page
//! webprobe scroll "https://example.com" --by-y 500
//! webprobe scroll "https://example.com" --to bottom
//!
//! # Execute JavaScript
//! webprobe eval "return document.title" --url "https://example.com"
//! ```
//!
//! ### Browser and Viewport Options
//!
//! ```bash
//! # Use Chrome instead of Firefox (default)
//! webprobe inspect "https://example.com" "h1" --browser chrome
//!
//! # Set custom viewport size
//! webprobe inspect "https://example.com" ".responsive" --viewport 375x667
//!
//! # Run in visible mode (not headless)
//! webprobe inspect "https://example.com" "h1" --headless false
//! ```
//!
//! ### Profile Management (Persistent Sessions)
//!
//! ```bash
//! # Create a named profile for session persistence
//! webprobe profile create my-profile --browser firefox
//!
//! # Use a profile (maintains cookies, localStorage)
//! webprobe inspect "https://app.com" ".dashboard" --profile my-profile
//!
//! # List all profiles
//! webprobe profile list
//!
//! # Delete a profile
//! webprobe profile delete my-profile
//! ```
//!
//! ### Daemon Mode with Persistent Tabs
//!
//! ```bash
//! # Start the daemon (enables browser pooling for 10x faster operations)
//! webprobe daemon start
//!
//! # Create a persistent tab for authentication workflows
//! webprobe type "https://app.com" "input#email" "user@example.com" --tab auth-session
//! webprobe type "https://app.com" "input#password" "password" --tab auth-session
//! webprobe click "https://app.com" "button[type='submit']" --tab auth-session
//! # The 'auth-session' tab maintains login state across commands!
//!
//! # Use profiles with tabs for complete isolation
//! webprobe inspect "https://api.com" ".data" --tab prod --profile production
//! webprobe inspect "https://api.com" ".data" --tab dev --profile development
//!
//! # List active tabs
//! webprobe tab list
//! webprobe tab list --profile production  # Filter by profile
//!
//! # Close a tab
//! webprobe tab close auth-session
//!
//! # Stop the daemon
//! webprobe daemon stop
//! ```
//!
//! ### Advanced Analysis
//!
//! ```bash
//! # Analyze element layout and spacing
//! webprobe analyze "https://example.com" ".card" --focus spacing
//!
//! # Analyze why elements might be wrapping
//! webprobe analyze "https://example.com" ".container" --focus wrapping
//!
//! # Find layout anomalies (hidden elements, overflow, etc.)
//! webprobe analyze "https://example.com" "body" --focus anomalies
//! ```
//!
//! ### JSON Output and Processing with jq
//!
//! ```bash
//! # Get element position as JSON
//! webprobe inspect "https://example.com" "h1" --format json
//!
//! # Extract just the position using jq
//! webprobe inspect "https://example.com" "h1" | jq '.position'
//! # Output: {"x": 100, "y": 50, "unit": "px"}
//!
//! # Get text content of all matching elements
//! webprobe inspect "https://example.com" ".card" --all | jq '.[].text_content'
//!
//! # Find elements wider than 500px
//! webprobe inspect "https://example.com" "div" --all | \
//!   jq '.[] | select(.size.width > 500) | {selector, width: .size.width}'
//!
//! # Check if element is visible (position and size > 0)
//! webprobe inspect "https://example.com" "#banner" | \
//!   jq 'if .position.x >= 0 and .position.y >= 0 and .size.width > 0 and .size.height > 0
//!       then "visible" else "hidden" end'
//!
//! # Count matching elements
//! webprobe inspect "https://example.com" ".item" --all | jq 'length'
//!
//! # Get computed style property
//! webprobe inspect "https://example.com" ".button" | \
//!   jq '.computed_styles.backgroundColor'
//!
//! # Check multiple elements and format output
//! webprobe inspect "https://example.com" ".nav-item" --all | \
//!   jq -r '.[] | "\(.selector) at (\(.position.x),\(.position.y)) - \(.text_content // "no text")"'
//!
//! # Script automation example
//! BUTTON_POS=$(webprobe inspect "https://example.com" "button#submit" | jq '.position')
//! if [ $(echo $BUTTON_POS | jq '.y') -gt 600 ]; then
//!   echo "Button is below the fold"
//! fi
//!
//! # Combine with other commands for workflow automation
//! # Check if login button exists before clicking
//! if webprobe inspect "https://app.com" "button#login" 2>/dev/null | jq -e '.size.width > 0' > /dev/null; then
//!   webprobe click "https://app.com" "button#login"
//! fi
//! ```
//!
//! ## Performance Tips
//!
//! - **Use daemon mode** for repeated operations (10x faster)
//! - **Use tabs** (`--tab`) to maintain browser state
//! - **Use profiles** for session isolation in testing
//! - **Default headless mode** is faster than visible mode
//! - **JSON + jq** for programmatic parsing in scripts
//!
//! ## Library Usage
//!
//! ```no_run
//! use webprobe::{Browser, BrowserType};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let browser = Browser::new(
//!     BrowserType::Firefox,
//!     None,  // No profile
//!     None,  // Default viewport
//!     true   // Headless
//! ).await?;
//!
//! let elements = browser.inspect_element(
//!     "https://example.com",
//!     "h1",
//!     webprobe::InspectionDepth::Shallow,
//!     false, // Single element
//!     None,  // First match
//!     false  // Don't require unique
//! ).await?;
//! # Ok(())
//! # }
//! ```

/// Browser manager for tab management and isolation
pub mod browser_manager;

/// Profile management for browser sessions
pub mod profile;

/// Type definitions for element information
pub mod types;

/// WebDriver browser control and automation
pub mod webdriver;

/// Automatic WebDriver process management
pub mod webdriver_manager;


pub use profile::ProfileManager;
pub use types::{
    BoundingBox, BoxModel, BoxSides, ContentBox, ElementInfo, InspectionDepth, LayoutInfo,
    OutputFormat, Position, Size, ViewportSize,
};
pub use webdriver::{Browser, BrowserType, ConsoleMessage};
