# webprobe

[![Crates.io](https://img.shields.io/crates/v/webprobe.svg)](https://crates.io/crates/webprobe)
[![Documentation](https://docs.rs/webprobe/badge.svg)](https://docs.rs/webprobe)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/karthikkolli/webprobe/blob/main/LICENSE)
[![CI](https://github.com/karthikkolli/webprobe/workflows/CI/badge.svg)](https://github.com/karthikkolli/webprobe/actions)

A CLI tool for LLMs to inspect web applications programmatically, providing layout/structure information without visual rendering.

## Overview

webprobe allows LLMs and automated tools to understand web page layouts by providing precise measurements, positions, and semantic information about elements. It supports multiple browsers (Firefox, Chrome) via WebDriver protocol.

## Features

- 🎯 **Element Inspection** - Get position, size, and properties of web elements
- 🔍 **Multiple Element Matching** - Find all, specific index, or expect single matches
- 📱 **Viewport Control** - Set custom viewport sizes for responsive testing
- 👤 **Profile Management** - Persistent and temporary browser profiles
- 🖱️ **Element Interaction** - Click elements, type text, scroll pages
- 💻 **JavaScript Execution** - Run arbitrary JavaScript and get results
- 🔬 **Layout Analysis** - Focused data gathering for debugging spacing, wrapping, and anomalies
- 🚀 **Performance** - Headless mode by default, optional visible mode
- 📊 **Flexible Output** - JSON or simple text format
- 🔄 **Daemon Mode** - Maintain browser sessions across commands for authentication workflows
- 📑 **Persistent Tabs** - Keep browser tabs alive for stateful operations
- ⚡ **Browser Pool** - Reuse browser instances for ~10x faster one-shot commands with daemon

## Installation

### Quick Install (No Rust Required)

#### macOS/Linux
```bash
# Download and install latest release
curl -fsSL https://raw.githubusercontent.com/karthikkolli/webprobe/main/install.sh | bash
```

#### Windows
```powershell
# Download latest release from GitHub
irm https://github.com/karthikkolli/webprobe/releases/latest/download/webprobe-windows.exe -OutFile webprobe.exe
# Add to PATH or move to a directory in PATH
```

#### Manual Download
Download pre-built binaries from the [releases page](https://github.com/karthikkolli/webprobe/releases):
- **macOS**: `webprobe-darwin-amd64` (Intel) or `webprobe-darwin-arm64` (Apple Silicon)
- **Linux**: `webprobe-linux-amd64` or `webprobe-linux-arm64`
- **Windows**: `webprobe-windows.exe`

After downloading:
```bash
# macOS/Linux: Make executable and move to PATH
chmod +x webprobe-*
sudo mv webprobe-* /usr/local/bin/webprobe

# Windows: Add to PATH or move to C:\Windows\System32
```

### Package Managers

#### Homebrew (macOS/Linux)
```bash
brew tap karthikkolli/webprobe
brew install webprobe
```

#### Scoop (Windows)
```powershell
scoop bucket add webprobe https://github.com/karthikkolli/webprobe-scoop
scoop install webprobe
```

#### Debian/Ubuntu (.deb)
```bash
# Download the .deb package
wget https://github.com/karthikkolli/webprobe/releases/latest/download/webprobe_amd64.deb
# Install
sudo dpkg -i webprobe_amd64.deb
```

#### RedHat/Fedora (.rpm)
```bash
# Download the .rpm package
wget https://github.com/karthikkolli/webprobe/releases/latest/download/webprobe.x86_64.rpm
# Install
sudo rpm -i webprobe.x86_64.rpm
```

### Install from Source (Rust Required)

If you have Rust installed, you can build from source:

```bash
# From crates.io
cargo install webprobe

# From GitHub
cargo install --git https://github.com/karthikkolli/webprobe

# Clone and build manually
git clone https://github.com/karthikkolli/webprobe.git
cd webprobe
cargo install --path .
```

### Prerequisites

webprobe requires a WebDriver-compatible browser and its driver. The tool **automatically starts and manages** the driver process for you.

<details>
<summary><b>🦊 Firefox Setup</b></summary>

1. Install Firefox browser
2. Install geckodriver:

```bash
# macOS
brew install geckodriver

# Linux
wget https://github.com/mozilla/geckodriver/releases/latest/download/geckodriver-v0.36.0-linux64.tar.gz
tar -xzf geckodriver-v0.36.0-linux64.tar.gz
sudo mv geckodriver /usr/local/bin/

# Windows
# Download from https://github.com/mozilla/geckodriver/releases
# Add to PATH
```
</details>

<details>
<summary><b>🔵 Chrome Setup</b></summary>

1. Install Chrome/Chromium browser
2. Install chromedriver:

```bash
# macOS
brew install chromedriver

# Linux
# Get the version that matches your Chrome
wget https://chromedriver.storage.googleapis.com/114.0.5735.90/chromedriver_linux64.zip
unzip chromedriver_linux64.zip
sudo mv chromedriver /usr/local/bin/

# Windows
# Download from https://chromedriver.chromium.org/
# Add to PATH
```
</details>

### Verify Installation

```bash
# Check webprobe version
webprobe version

# Test with a simple inspection
webprobe inspect "https://example.com" "h1"
```

## Updating webprobe

### Check for Updates

```bash
# Check if a new version is available
webprobe update

# Auto-install update if available
webprobe update --install
```

### Update Methods

Depending on how you installed webprobe, use the appropriate method:

#### Quick Update (curl/wget installer)
```bash
curl -fsSL https://raw.githubusercontent.com/karthikkolli/webprobe/main/install.sh | bash
```

#### Package Managers
```bash
# Homebrew
brew upgrade webprobe

# Scoop (Windows)
scoop update webprobe

# APT (Debian/Ubuntu)
sudo apt update && sudo apt upgrade webprobe

# YUM/DNF (RedHat/Fedora)
sudo dnf upgrade webprobe
# or
sudo yum update webprobe
```

#### Rust/Cargo
```bash
cargo install webprobe --force
```

### Version History

See [CHANGELOG.md](CHANGELOG.md) for detailed version history and release notes.

## MCP Server Setup (For Claude Code)

webprobe can run as an MCP (Model Context Protocol) server for seamless integration with Claude Code, providing direct access to browser automation tools.

### Installation

1. Build with MCP support:
```bash
cargo install webprobe --features mcp
# Or from source:
cargo build --release --features mcp
```

2. Add to Claude Code configuration (`~/.config/claude/claude.json`):
```json
{
  "mcpServers": {
    "webprobe": {
      "command": "/path/to/webprobe",
      "args": ["mcp-server"],
      "env": {}
    }
  }
}
```

3. Restart Claude Code to load the MCP server

### Available MCP Tools

When configured, Claude will have access to these tools:
- `webprobe_inspect` - Inspect elements with position, size, and styles
- `webprobe_click` - Click on web elements
- `webprobe_type_text` - Type text into input fields
- `webprobe_scroll` - Scroll pages
- `webprobe_eval` - Execute JavaScript
- `webprobe_analyze` - Analyze layout issues (spacing, overflow, alignment)
- `webprobe_layout` - Get detailed box model information
- `webprobe_tab_create` - Create persistent browser tabs
- `webprobe_tab_close` - Close tabs
- `webprobe_tab_list` - List active tabs

### MCP Usage Example

```javascript
// Claude can use these tools directly:
await webprobe_tab_create({name: "session", browser: "firefox"})
await webprobe_type_text({url: "https://app.com", selector: "#email", text: "user@example.com", tab: "session"})
await webprobe_click({url: "https://app.com", selector: "button[type='submit']", tab: "session"})
// Tab maintains login state across commands!
```

For detailed MCP usage, see [MCP_USAGE.md](MCP_USAGE.md).

## Using with LLMs (Manual Integration)

If you want to use webprobe with any LLM (ChatGPT, Claude, etc.) without MCP, you can provide the LLM with tool documentation:

1. Share the [LLM_WEBPROBE_PROMPT.txt](LLM_WEBPROBE_PROMPT.txt) file with your LLM
2. The LLM will understand how to use webprobe commands effectively
3. Optionally start the daemon for better performance:
   ```bash
   webprobe daemon start  # Enables browser pooling and persistent tabs
   ```

This approach works with any LLM that can execute shell commands, providing the same powerful browser automation capabilities.

## Quick Start

### Basic Usage

No need to manually start WebDriver! webprobe automatically starts geckodriver or chromedriver when needed.

```bash
# Inspect an element
webprobe inspect "https://www.google.com" "textarea"

# Output:
# {
#   "selector": "textarea",
#   "browser": "Firefox",
#   "position": {"x": 134.0, "y": 179.0, "unit": "px"},
#   "size": {"width": 443.0, "height": 50.0, "unit": "px"},
#   ...
# }
```

## Commands

### Inspect

Find and analyze elements on a webpage:

```bash
# Basic inspection
webprobe inspect <URL> <SELECTOR>

# Options:
#   --browser <firefox|chrome>  # Browser to use (default: firefox)
#   --profile <name>            # Use named profile
#   --format <json|simple>      # Output format (default: json)
#   --all                       # Return all matching elements
#   --index <n>                 # Return element at index n
#   --expect-one               # Error if multiple elements found
#   --viewport WIDTHxHEIGHT    # Set viewport size (e.g., 1920x1080)
#   --headless <true|false>    # Run in headless mode (default: true)
```

#### Examples

```bash
# Find all buttons on a page
webprobe inspect "https://example.com" "button" --all

# Get the third link
webprobe inspect "https://example.com" "a" --index 2

# Mobile viewport testing
webprobe inspect "https://example.com" "nav" --viewport 375x667

# Use visible browser for debugging
webprobe inspect "https://example.com" "form" --headless false
```

#### Multiple Element Handling

When multiple elements match your selector, webprobe helps you avoid debugging the wrong element:

```bash
# Default: Returns first element with warning
webprobe inspect "https://example.com" ".nav"
# Output includes: "metadata": {"total_matches": 3, "warning": "3 elements match..."}

# See all matching elements
webprobe inspect "https://example.com" ".nav" --all

# Target specific element by index
webprobe inspect "https://example.com" ".nav" --index 1  # Get second nav
```

### Click

Click an element on a webpage:

```bash
webprobe click <URL> <SELECTOR> [OPTIONS]

# Options:
#   --index <n>                # Click element at index n
#   --browser <firefox|chrome> # Browser to use
#   --profile <name>          # Use named profile
#   --viewport WIDTHxHEIGHT   # Set viewport size
```

### Type

Type text into an input field:

```bash
webprobe type <URL> <SELECTOR> <TEXT> [OPTIONS]

# Options:
#   --clear                    # Clear field before typing
#   --browser <firefox|chrome> # Browser to use
#   --profile <name>          # Use named profile

# Example: Search on Google
webprobe type "https://google.com" "textarea" "rust programming"
```

### Scroll

Scroll the page or an element:

```bash
webprobe scroll <URL> [OPTIONS]

# Options:
#   --selector <sel>           # Element to scroll (default: window)
#   --by-x <pixels>           # Scroll horizontally
#   --by-y <pixels>           # Scroll vertically
#   --to <position>           # Scroll to: top, bottom, or x,y

# Examples:
webprobe scroll "https://example.com" --by-y 500
webprobe scroll "https://example.com" --to bottom
webprobe scroll "https://example.com" --to "0,1000"
```

### Analyze

Gather focused context data for diagnosing layout issues:

```bash
webprobe analyze <URL> <SELECTOR> [OPTIONS]

# Options:
#   --focus <MODE>             # What to analyze: spacing, wrapping, anomalies, all
#   --proximity <pixels>       # Include elements within distance (default: 100)
#   --index <n>               # Analyze element at index n when multiple match
#   --browser <firefox|chrome> # Browser to use
#   --profile <name>          # Use named profile
#   --format <json|simple>    # Output format

# Focus Modes:
#   spacing    - Adjacent elements, gaps, margin collapse
#   wrapping   - Container/child dimensions, row calculations
#   anomalies  - Unusual properties, invisible elements, viewport issues
#   all        - Comprehensive element information

# Examples:
# Debug why there's extra space above navigation
webprobe analyze "https://example.com" "nav" --focus spacing

# Figure out why cards are wrapping to next line
webprobe analyze "https://example.com" ".card-container" --focus wrapping

# Find hidden or broken elements
webprobe analyze "https://example.com" "body" --focus anomalies
```

### Eval

Execute JavaScript in the browser context:

```bash
webprobe eval <CODE> [OPTIONS]

# Options:
#   --url <URL>               # URL to navigate to first
#   --browser <firefox|chrome> # Browser to use
#   --format <json|simple>    # Output format

# Examples:
webprobe eval "return document.title" --url "https://example.com"
webprobe eval "return document.querySelectorAll('a').length" --url "https://example.com"
webprobe eval "return window.innerWidth" --url "https://example.com"
```

### Profile Management

Manage browser profiles for persistent state:

```bash
# Create a profile
webprobe profile create <NAME> --browser <firefox|chrome>

# List profiles
webprobe profile list

# Delete a profile
webprobe profile delete <NAME>

# Clean up old temporary profiles
webprobe profile cleanup --older-than <DAYS>
```

### Daemon Mode

For workflows requiring persistent browser sessions (like authentication), use daemon mode.

#### Performance Benefits
When daemon is running, webprobe uses a **browser pool** for one-shot commands:
- First command: ~2-3 seconds (browser startup)
- Subsequent commands: ~0.3 seconds (reuses pooled browser)
- Up to 10x faster for repeated operations
- Automatic cleanup of idle browsers after 5 minutes

```bash
# Start the daemon in the background
webprobe daemon start
# Output: Daemon started successfully
# Log file: ~/Library/Caches/webprobe-daemon.log

# Check daemon status
webprobe daemon status
# Output: Daemon is running

# One-shot commands now use browser pool (much faster!)
webprobe inspect "https://site.com" ".element"  # Fast!
webprobe inspect "https://site.com" ".other"    # Even faster!

# Use persistent tabs with --tab flag
webprobe type "https://app.com" "input[name='email']" "user@example.com" --tab auth
webprobe type "https://app.com" "input[name='password']" "secret" --tab auth
webprobe click "https://app.com" "button[type='submit']" --tab auth
# The 'auth' tab persists across all commands, maintaining login state

# List active tabs
webprobe tab list
# Output: auth - https://app.com

# List tabs filtered by profile
webprobe tab list --profile mentor
# Output: Shows only tabs using the 'mentor' profile

# Close a specific tab
webprobe tab close auth

# Stop the daemon
webprobe daemon stop
```

#### Key Benefits

- **Authentication Workflows**: Login once, reuse the session
- **Stateful Testing**: Maintain application state across commands
- **Performance**: Avoid browser startup overhead for each command
- **Session Management**: Multiple named tabs for different contexts

## Output Formats

### JSON Format (Default)

```json
{
  "selector": "button",
  "browser": "Firefox",
  "position": {
    "x": 100.0,
    "y": 50.0,
    "unit": "px"
  },
  "size": {
    "width": 200.0,
    "height": 40.0,
    "unit": "px"
  },
  "computed_styles": {
    "display": "block",
    "tag": "button"
  },
  "text_content": "Click Me",
  "children_count": 0
}
```

### Simple Format

```
button: button element at (100, 50) 200x40px
  Text: Click Me
```

## Real-World Examples

### Authentication Workflow

```bash
# Start daemon for persistent sessions
webprobe daemon start

# Login workflow using persistent tab
webprobe type "http://localhost:3000" "input[type='email']" "admin@example.com" --tab session --browser chrome
webprobe type "http://localhost:3000" "input[type='password']" "mypassword" --tab session --browser chrome
webprobe click "http://localhost:3000" "button[type='submit']" --tab session --browser chrome

# Now authenticated - continue using the same tab
webprobe inspect "http://localhost:3000/dashboard" ".user-data" --tab session --browser chrome
# Returns authenticated content

# Clean up
webprobe daemon stop
```

### Parallel Testing with Profile Isolation

```bash
# Start daemon for persistent sessions
webprobe daemon start

# Test production environment (in one terminal)
webprobe inspect "https://httpbin.org/cookies/set?env=production" "body" --tab prod-tab --profile production
webprobe inspect "https://httpbin.org/cookies" "body" --tab prod-tab --profile production
# Shows: {"cookies": {"env": "production"}}

# Test staging environment (in another terminal, simultaneously)
webprobe inspect "https://httpbin.org/cookies/set?env=staging" "body" --tab staging-tab --profile staging  
webprobe inspect "https://httpbin.org/cookies" "body" --tab staging-tab --profile staging
# Shows: {"cookies": {"env": "staging"}}

# List tabs by profile
webprobe tab list --profile production
# Output: prod-tab [profile: production] - https://httpbin.org/cookies

webprobe tab list --profile staging  
# Output: staging-tab [profile: staging] - https://httpbin.org/cookies

# Clean up
webprobe daemon stop
```

Profiles provide complete session isolation - cookies, local storage, and authentication state are separate between profiles, allowing parallel testing of different environments or user sessions.

### Finding Google Search Bar

```bash
# Start chromedriver
chromedriver &

# Find search bar position on desktop
webprobe inspect "https://www.google.com" "textarea" --browser chrome

# Result: Position (134, 179), Size 443x50px

# Test mobile layout
webprobe inspect "https://www.google.com" "textarea" \
  --browser chrome --viewport 375x667

# Result: Position (68, 179), Size 319x50px
```

## Use Cases

- **LLM Web Navigation** - Provide spatial understanding for AI agents
- **Automated Testing** - Verify element positions and sizes
- **Responsive Design Testing** - Check layouts at different viewport sizes
- **Accessibility Testing** - Analyze element structure and properties
- **Web Scraping** - Find elements programmatically without visual rendering

## Architecture

webprobe uses:
- **WebDriver Protocol** - Standard browser automation protocol
- **Headless Browsers** - Fast, resource-efficient operation
- **Profile Management** - Persistent and temporary browser states
- **Async/Await** - Efficient concurrent operations

## Troubleshooting

### WebDriver Not Found
```
Error: geckodriver not found in PATH
```
**Solution**: Install geckodriver or chromedriver using the installation instructions above. webprobe will automatically start them when needed.

### No Elements Found
```
Error: No elements found matching selector: [selector]
```
**Solution**: Verify selector is correct, element may be dynamically loaded

### Profile Issues
```
Error: Profile 'name' already exists
```
**Solution**: Delete existing profile or use a different name

## Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=webprobe=debug cargo run -- inspect "https://example.com" "body"

# Format code
cargo fmt

# Check lints
cargo clippy
```

## Contributing

Contributions are welcome! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Quick Start for Contributors

```bash
# Fork and clone the repository
git clone https://github.com/your-username/webprobe.git
cd webprobe

# Create a feature branch from develop
git checkout -b feature/your-feature develop

# Make your changes and test
cargo test
cargo fmt
cargo clippy

# Commit and push
git commit -m "Add your feature"
git push origin feature/your-feature
```

## License

MIT License - See LICENSE file for details

## Acknowledgments

- Built with [fantoccini](https://github.com/jonhoo/fantoccini) WebDriver client
- Uses [clap](https://github.com/clap-rs/clap) for CLI parsing
- Async runtime by [tokio](https://tokio.rs/)