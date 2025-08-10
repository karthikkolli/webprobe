# Changelog

All notable changes to webprobe will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-01-09

### Added
- Initial release of webprobe
- Core inspection functionality for web elements
- Support for Firefox and Chrome browsers via WebDriver
- Persistent browser sessions with daemon mode
- Browser pool for 10x faster repeated operations
- Named tabs for maintaining state across commands
- Profile management for session isolation
- MCP (Model Context Protocol) server for Claude Code integration
- Automatic WebDriver process management
- Console log capture for debugging
- Layout analysis tools (spacing, overflow, alignment)
- JavaScript execution in browser context
- Multiple output formats (JSON, simple text)
- Viewport control for responsive testing
- Version checking and update command
- Pre-built binaries for major platforms
- Installation via package managers (Homebrew, APT, RPM)

### Features
- **Element Inspection**: Get position, size, computed styles, and text content
- **Interaction**: Click elements, type text, scroll pages
- **Session Management**: Persistent tabs maintain cookies and localStorage
- **Profile Isolation**: Run parallel tests with separate browser profiles
- **Performance**: Browser pooling reduces command latency to ~0.3s
- **MCP Integration**: Direct tool access in Claude Code
- **Auto-update**: Built-in version checking and update notifications

### Supported Platforms
- macOS (Intel & Apple Silicon)
- Linux (x64 & ARM64)
- Windows (x64)

### Dependencies
- Requires geckodriver (Firefox) or chromedriver (Chrome)
- Rust 1.70+ for building from source

## [Unreleased]

### Planned Features
- WebDriver BiDi support when ecosystem matures
- Enhanced Shadow DOM detection
- Network request interception
- Performance metrics collection
- Screenshot capture
- HAR file generation
- Cookie management commands
- Local storage manipulation
- Multi-tab coordination

---

For upgrade instructions, see the [README](README.md#updating-webprobe).