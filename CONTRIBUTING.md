# Contributing to webprobe

First off, thank you for considering contributing to webprobe! It's people like you that make webprobe such a great tool.

## Code of Conduct

This project and everyone participating in it is governed by our Code of Conduct. By participating, you are expected to uphold this code. Please be respectful and constructive in all interactions.

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check existing issues as you might find out that you don't need to create one. When you are creating a bug report, please include as many details as possible using our issue template.

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. Create an issue using the feature request template and provide the following information:

- Use a clear and descriptive title
- Provide a step-by-step description of the suggested enhancement
- Provide specific examples to demonstrate the steps
- Describe the current behavior and explain why this enhancement would be useful

### Pull Requests

1. Fork the repo and create your branch from `develop`
2. If you've added code that should be tested, add tests
3. If you've changed APIs, update the documentation
4. Ensure the test suite passes
5. Make sure your code follows the existing style
6. Issue that pull request!

## Development Process

We use GitHub flow, so all changes happen through pull requests:

1. Fork the repo and create your branch from `develop`
2. Make your changes
3. Test your changes thoroughly
4. Create a pull request to `develop` branch

### Branch Naming

- `feature/description` - New features
- `fix/description` - Bug fixes
- `docs/description` - Documentation changes
- `refactor/description` - Code refactoring
- `test/description` - Test additions or changes

### Setup Development Environment

```bash
# Clone your fork
git clone https://github.com/your-username/webprobe.git
cd webprobe

# Add upstream remote
git remote add upstream https://github.com/karthikkolli/webprobe.git

# Install dependencies
cargo build

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- inspect "https://example.com" "h1"
```

### Testing

- Write unit tests for new functionality
- Ensure all tests pass: `cargo test`
- Add integration tests for new commands
- Test with both Firefox and Chrome

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix any warnings
- Follow Rust naming conventions
- Add documentation comments for public APIs
- Keep functions small and focused

### Documentation

- Update README.md if adding new features
- Add inline documentation for complex logic
- Update .claude/documentation/TOOL_DOCUMENTATION.txt for LLM usage
- Include examples in documentation

### Commit Messages

- Use the present tense ("Add feature" not "Added feature")
- Use the imperative mood ("Move cursor to..." not "Moves cursor to...")
- Limit the first line to 72 characters or less
- Reference issues and pull requests liberally after the first line

Example:
```
Add browser pool for performance optimization

- Implement connection pooling for browser instances
- Add automatic cleanup for stale browsers
- Reduce command latency by 10x

Fixes #123
```

## Release Process

1. All features are developed in feature branches off `develop`
2. Pull requests are merged to `develop` after review
3. When ready for release, `develop` is merged to `main`
4. Tags are created on `main` for releases (e.g., `v0.1.0`)
5. GitHub Actions automatically publishes to crates.io

## Questions?

Feel free to open an issue with the question label or reach out to the maintainers.

Thank you for contributing!