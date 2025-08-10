# MCP Server Usage with Claude Code

## Overview

The webprobe MCP server provides browser automation tools directly to Claude Code, enabling persistent browser sessions and efficient web automation workflows.

## Installation

1. Build webprobe with MCP support:
```bash
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

## Available Tools

When configured, Claude Code will have access to these tools:

### webprobe_inspect
Inspect web elements to get position, size, and computed styles
- **url**: URL to navigate to
- **selector**: CSS selector for the element
- **all**: Return all matching elements (optional)
- **index**: Index of element to return (optional)
- **tab**: Named tab for persistent state (optional)
- **browser**: Browser type - firefox or chrome (optional)
- **profile**: Profile name for session persistence (optional)

### webprobe_click
Click on a web element
- **url**: URL to navigate to
- **selector**: CSS selector for the element
- **tab**: Named tab for persistent state (optional)

### webprobe_type_text
Type text into an input element
- **url**: URL to navigate to
- **selector**: CSS selector for the input
- **text**: Text to type
- **clear**: Clear field before typing (optional)
- **tab**: Named tab for persistent state (optional)

### webprobe_scroll
Scroll the web page
- **url**: URL to navigate to
- **by_x**: Pixels to scroll horizontally (optional)
- **by_y**: Pixels to scroll vertically (optional)
- **to**: Scroll to "top" or "bottom" (optional)
- **tab**: Named tab for persistent state (optional)

### webprobe_eval
Execute JavaScript code in the browser
- **url**: URL to navigate to
- **script**: JavaScript code to execute
- **tab**: Named tab for persistent state (optional)

### webprobe_tab_create
Create a persistent browser tab for maintaining state
- **name**: Name for the tab
- **browser**: Browser type (optional)
- **profile**: Profile name (optional)
- **viewport**: Viewport size like "1920x1080" (optional)
- **headless**: Run in headless mode (optional)

### webprobe_tab_close
Close a persistent browser tab
- **name**: Name of the tab to close

### webprobe_tab_list
List all active browser tabs

### webprobe_analyze
Analyze page context with focus on specific aspects
- **url**: URL to navigate to
- **selector**: CSS selector for the element
- **focus**: Focus area - spacing, overflow, alignment, z-index, or all (optional)
- **proximity**: Include elements within this distance in pixels (optional, default: 100)
- **index**: Index of element when multiple match (optional)
- **tab**: Named tab for persistent state (optional)

### webprobe_layout
Analyze element layout with box model details
- **url**: URL to navigate to
- **selector**: CSS selector for the element
- **depth**: Maximum depth to traverse (optional, default: 2)
- **max_elements**: Maximum number of elements to analyze (optional, default: 50)
- **wait_stable**: Wait for layout to stabilize (optional)
- **detect_shadow**: Detect shadow DOM (optional)
- **tab**: Named tab for persistent state (optional)

## Usage Examples

### One-Shot Operations
```
Claude: I'll inspect that element for you
> webprobe_inspect(url="https://example.com", selector="h1")
```

### Authentication Workflow with Persistent Tabs
```
Claude: I'll log into the application using a persistent tab
> webprobe_tab_create(name="auth-session", browser="firefox")
> webprobe_type_text(url="https://app.com", selector="#email", text="user@example.com", tab="auth-session")
> webprobe_type_text(url="https://app.com", selector="#password", text="password", tab="auth-session")
> webprobe_click(url="https://app.com", selector="button[type='submit']", tab="auth-session")
# The auth-session tab maintains login state!
> webprobe_inspect(url="https://app.com/dashboard", selector=".user-data", tab="auth-session")
```

### Parallel Testing with Profiles
```
Claude: I'll test both environments in parallel
> webprobe_tab_create(name="prod", profile="production")
> webprobe_tab_create(name="staging", profile="staging")
> webprobe_inspect(url="https://api.com", selector=".data", tab="prod")
> webprobe_inspect(url="https://staging-api.com", selector=".data", tab="staging")
```


## Performance Tips

1. **Use tabs** for any multi-step workflow
2. **Create profiles** for different environments (dev, staging, prod)
3. **Keep MCP server running** during development sessions
4. **Use headless mode** (default) for faster operations
5. **Close unused tabs** to free memory

## Troubleshooting

### MCP Server Not Starting
- Check that webprobe was built with `--features mcp`
- Verify the path in claude.json is correct
- Check logs in `~/.config/claude/logs/`

### Browser Connection Issues
- Ensure geckodriver/chromedriver are installed
- Check that ports 4444 (Firefox) and 9515 (Chrome) are available
- Try running with `--no-headless` to see browser window

### Tab State Issues
- Use `webprobe_tab_list()` to see active tabs
- Close and recreate tabs if state is corrupted
- Each tab maintains its own browser instance

## Development Workflow

1. Start Claude Code with webprobe MCP server configured
2. Create persistent tabs for different testing scenarios
3. Use tabs throughout your development session
4. Tabs persist across multiple Claude conversations
5. Close tabs when done to free resources

## Notes

- The MCP server runs as a background process managed by Claude Code
- Browser instances are pooled for efficiency
- Tabs are isolated - cookies/localStorage don't share between tabs
- Profiles provide additional isolation for parallel testing
- All WebDriver errors are reported back to Claude for debugging