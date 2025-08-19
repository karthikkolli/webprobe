#\!/bin/bash

# Install script for webprobe that handles macOS security

BINARY="./target/release/webprobe"
DEST="$HOME/.bin/webprobe"

if [ \! -f "$BINARY" ]; then
    echo "Error: webprobe binary not found at $BINARY"
    echo "Please run 'cargo build --release' first"
    exit 1
fi

echo "Installing webprobe to $DEST..."

# Create .bin directory if it doesn't exist
mkdir -p "$HOME/.bin"

# Remove old binary if it exists
if [ -f "$DEST" ]; then
    rm "$DEST"
fi

# Copy the binary
cp "$BINARY" "$DEST"

# Remove quarantine and other extended attributes
xattr -cr "$DEST" 2>/dev/null || true

# Re-sign with ad-hoc signature for macOS
if [[ "$OSTYPE" == "darwin"* ]]; then
    codesign --force --deep -s - "$DEST" 2>/dev/null || true
fi

# Make sure it's executable
chmod +x "$DEST"

# Test the installation
if "$DEST" version > /dev/null 2>&1; then
    echo "✅ webprobe installed successfully\!"
    "$DEST" version
else
    echo "⚠️  Installation completed but webprobe may not work correctly"
    echo "Try running: spctl --add $DEST"
    echo "Or move the binary to /usr/local/bin instead"
fi
