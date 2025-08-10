#!/bin/bash
set -e

# webprobe installer script
# This script downloads and installs the latest webprobe binary

REPO="karthikkolli/webprobe"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="webprobe"

# Check if webprobe is already installed
if command -v webprobe >/dev/null 2>&1; then
    CURRENT_VERSION=$(webprobe version 2>/dev/null | grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "unknown")
    echo "webprobe is already installed (version: $CURRENT_VERSION)"
    echo "This will upgrade to the latest version."
    read -p "Continue? (y/n) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Installation cancelled."
        exit 0
    fi
fi

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Map to binary naming convention
case "$OS" in
    darwin)
        OS_NAME="darwin"
        ;;
    linux)
        OS_NAME="linux"
        ;;
    *)
        echo "Error: Unsupported OS: $OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64)
        ARCH_NAME="amd64"
        ;;
    aarch64|arm64)
        ARCH_NAME="arm64"
        ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

BINARY_FILE="webprobe-${OS_NAME}-${ARCH_NAME}"

echo "Installing webprobe for ${OS_NAME}-${ARCH_NAME}..."

# Get latest release URL
LATEST_RELEASE=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_RELEASE" ]; then
    echo "Error: Could not fetch latest release"
    exit 1
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_RELEASE}/${BINARY_FILE}"

echo "Downloading from: $DOWNLOAD_URL"

# Create temp directory
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Download binary
curl -L -o "$TMP_DIR/$BINARY_NAME" "$DOWNLOAD_URL"

# Make executable
chmod +x "$TMP_DIR/$BINARY_NAME"

# Check if we need sudo
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
else
    echo "Installing to $INSTALL_DIR (requires sudo)..."
    sudo mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
fi

# Verify installation
if command -v webprobe >/dev/null 2>&1; then
    NEW_VERSION=$(webprobe version | grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+' | head -1)
    echo "✅ webprobe installed successfully!"
    echo "Version: $NEW_VERSION"
    echo ""
    echo "Next steps:"
    echo "1. Install browser driver (geckodriver or chromedriver)"
    echo "   macOS: brew install geckodriver"
    echo "   Linux: See https://github.com/karthikkolli/webprobe#prerequisites"
    echo ""
    echo "2. Test installation:"
    echo "   webprobe inspect \"https://example.com\" \"h1\""
    echo ""
    echo "To check for updates in the future:"
    echo "   webprobe update"
    echo ""
    echo "To update webprobe:"
    echo "   curl -fsSL https://raw.githubusercontent.com/karthikkolli/webprobe/main/install.sh | bash"
else
    echo "⚠️  Installation completed but webprobe not found in PATH"
    echo "Add $INSTALL_DIR to your PATH:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
fi