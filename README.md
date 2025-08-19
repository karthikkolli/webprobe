# webprobe

A CLI tool that gives LLMs eyes to see how web pages actually render during frontend development. Built for modern SPAs where you need to click through navigation to reach the component you're testing.

## Core Purpose

**For**: Frontend development with LLMs (Claude Code, Cursor, etc.)  
**Problem**: LLMs can write CSS/React/Vue but can't see if elements overlap, overflow, or misalign  
**Solution**: webprobe provides exact pixel positions, computed styles, and layout diagnostics

**Why webprobe over screenshots**: Precise measurements, automatic issue detection, and maintaining state through SPA navigation

## Key Features

- **SPA Navigation Support** - Click through to any component, maintain state across hot-reloads
- **Precise Measurements** - Exact pixel positions, dimensions, computed CSS values
- **Layout Debugging** - Detect overflow, margin collapse, z-index issues automatically
- **Session Persistence** - Profiles and tabs maintain auth/state across commands
- **High Performance** - Daemon with browser pool for instant responses (~0.3s)
- **Smart Detection** - Find forms, navigation, tables automatically
- **Responsive Testing** - Test layouts at different viewport sizes

## Installation

### Prerequisites
1. **Browser**: Chrome or Firefox installed
2. **WebDriver**: ChromeDriver or GeckoDriver (must match browser version)
   - macOS: `brew install chromedriver` or `brew install geckodriver`
   - Linux/Windows: Download from official sites
   - Will auto-start if found in PATH

### Quick Install
```bash
# From crates.io
cargo install webprobe

# From source
git clone https://github.com/karthikkolli/webprobe
cd webprobe
cargo install --path .
```

### Verify Installation
```bash
# Start the daemon (required for all operations)
webprobe daemon start --browser chrome

# Test inspection
webprobe inspect "http://localhost:3000" "h1"
```

## Quick Start

### 1. Start the Daemon (Required)
```bash
# Start daemon with Chrome (recommended)
webprobe daemon start --browser chrome

# Or with Firefox
webprobe daemon start --browser firefox

# Check daemon status
webprobe daemon status
```

### 2. For Modern SPAs (React, Vue, Angular)

Most modern apps don't have direct URLs to every component. You need to maintain state:

```bash
# Create a profile for your development session
webprobe profile create dev --browser chrome

# Navigate to the component you're testing
webprobe inspect "http://localhost:3000" "body" --profile dev --tab main
webprobe click "" ".nav-products" --profile dev --tab main
webprobe click "" ".product-card:first" --profile dev --tab main

# Now test your component (empty URL = stay on current page)
webprobe inspect "" ".product-details" --profile dev --tab main
webprobe analyze "" ".price-container" --focus spacing --profile dev --tab main

# After hot-reload, check if still on the right page
webprobe find-text "" "Product Details" --profile dev --tab main
# If not, re-navigate using the same clicks above
```

### 3. For Static Sites / Direct URLs

If you can navigate directly to what you're testing:

```bash
# Simple one-shot commands (no profile needed)
webprobe inspect "http://localhost:3000/about.html" ".content"
webprobe analyze "http://localhost:3000/contact.html" ".form" --focus wrapping
```

## Important: Profile & Tab Architecture

**Key Rule**: `--tab` requires `--profile`. This is by design for session isolation.

```bash
# ❌ Wrong - tab without profile
webprobe inspect "http://localhost:3000" ".nav" --tab main

# ✅ Correct - profile + tab together
webprobe inspect "http://localhost:3000" ".nav" --profile dev --tab main
```

## Common Use Cases

### Debug Responsive Layouts
```bash
# Test at different breakpoints
webprobe inspect "http://localhost:3000" ".grid" --viewport 375x667   # Mobile
webprobe inspect "http://localhost:3000" ".grid" --viewport 768x1024  # Tablet
webprobe inspect "http://localhost:3000" ".grid" --viewport 1920x1080 # Desktop
```

### Find Layout Problems
```bash
# Check for overflow
webprobe analyze "http://localhost:3000" ".container" --focus wrapping

# Debug margins
webprobe analyze "http://localhost:3000" ".header" --focus spacing

# Find z-index issues
webprobe analyze "http://localhost:3000" "body" --focus anomalies
```

### Handle Dynamic Content
```bash
# Wait for elements to appear
webprobe wait-navigation "http://localhost:3000" --timeout 10
webprobe wait-idle "http://localhost:3000" --timeout 10000
```

## All Commands

### Core Commands
- `inspect` - Get element measurements and properties
- `analyze` - Diagnose layout issues with suggested fixes
- `detect` - Find forms, navigation, tables automatically
- `find-text` - Search elements by text content
- `click` - Click elements
- `type` - Enter text into inputs
- `scroll` - Scroll page or elements

### Waiting & Navigation
- `wait-navigation` - Wait for page changes
- `wait-idle` - Wait for network to settle

### Session Management
- `daemon start/stop` - Manage background daemon
- `tab list/close` - Manage persistent tabs
- `status --tab NAME` - Check session status

### Advanced
- `eval` - Execute JavaScript (requires `--unsafe-eval`)
- `batch` - Run multiple commands in sequence
- `screenshot` - Capture page images
- `iframe` - Inspect iframe content (same-origin only)
- `layout` - Get detailed box model

### Experimental
- `diagnose` - High-level issue detection
- `validate` - Accessibility/SEO checks
- `compare` - Diff two pages

## Options

- `--browser chrome|firefox` - Choose browser (for daemon start, default: chrome)
- `--viewport WIDTHxHEIGHT` - Set viewport size
- `--format json|simple` - Output format (default: json)
- `--headless true|false` - Run headlessly (default: true)
- `--tab NAME` - Use persistent tab for session state
- `--all` - Return all matching elements
- `--index N` - Return Nth element
- `--debug` - Show debug information

## JSON Output Format

### Success Response
```json
{
  "selector": ".card",
  "position": {"x": 10, "y": 50},
  "size": {"width": 300, "height": 200},
  "computed_styles": {
    "display": "flex",
    "position": "relative"
  },
  "visible": true,
  "in_viewport": true
}
```

### Error Response
```json
{
  "error": true,
  "message": "No elements found matching selector: .missing",
  "exit_code": 2
}
```

### Analyze Response with Fix
```json
{
  "diagnosis": "Cards overflow container on mobile",
  "confidence": 0.92,
  "evidence": [
    "container.width: 320px",
    "card.width: 340px"
  ],
  "suggested_fix": "Use width: 100%; box-sizing: border-box;"
}
```

## Security Notes

- Password fields are auto-redacted as `[REDACTED]` in output
- Profiles stored in `~/.webprobe/profiles/` (unencrypted)
- Use `--unsafe-eval` only with trusted code
- Empty URL (`""`) means stay on current page with `--tab`

## Limitations

- **No Shadow DOM piercing** - Selectors can't reach into shadow roots
- **Cross-origin iframes** - Security restriction, same-origin only
- **Virtualized lists** - May need scrolling to render all items
- **Animations** - Captures current frame only

## Troubleshooting

### WebDriver Connection Failed
- Ensure ChromeDriver/GeckoDriver matches browser version
- Check ports 4444 (Firefox) or 9515 (Chrome) aren't in use

### Element Not Found
- Element in iframe? Use `iframe` command
- Lazy loaded? Use `wait-idle` first  
- Shadow DOM? Not supported
- Virtualized? Scroll first

### Session Lost
- Ensure daemon is running: `webprobe daemon status`
- Use `--tab` consistently for all commands in a session
- Check tab status with `webprobe status --tab NAME`
- Remember: empty URL (`""`) means stay on current page

## Integration

### For LLMs (Claude Code, Cursor, etc.)
Share the [`LLM_WEBPROBE_PROMPT.txt`](LLM_WEBPROBE_PROMPT.txt) file with your LLM for complete command documentation.

## Performance

- **Daemon architecture**: Browser pool provides instant responses
- **One-shot operations**: Reuse pooled browsers (~0.3s vs several seconds)
- **Persistent tabs**: Zero overhead for subsequent commands
- **Auto-retry**: Handles dynamic content automatically
- **Browser pool**: Maintains up to 3 instances for parallel operations

## Testing

```bash
# Run all tests
cargo test

# Daemon tests need single threading
cargo test --test daemon_integration_test -- --test-threads=1
```

## License

MIT License - See [LICENSE](LICENSE) file

## Contributing

Contributions welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

## Links

- [Documentation](https://docs.rs/webprobe)
- [Crates.io](https://crates.io/crates/webprobe)
- [GitHub](https://github.com/karthikkolli/webprobe)
- [Issue Tracker](https://github.com/karthikkolli/webprobe/issues)