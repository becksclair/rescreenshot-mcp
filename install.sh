#!/bin/bash
set -e

# screenshot-mcp Linux Installer
# Downloads the latest release from GitHub and installs it to ~/.local/bin

REPO="username/screenshot-mcp"
INSTALL_DIR="$HOME/.local/bin"
BINARY_NAME="screenshot-mcp"

echo "Installing screenshot-mcp..."

# Ensure install directory exists
mkdir -p "$INSTALL_DIR"

# Detect architecture
ARCH=$(uname -m)
if [ "$ARCH" != "x86_64" ]; then
    echo "Error: Only x86_64 architecture is currently supported."
    exit 1
fi

# Fetch latest release URL (using GitHub API would be better, but simple download is easier for now)
# NOTE: This assumes the binary naming convention from release.yml
DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/screenshot-mcp-linux-x86_64"

echo "Downloading from $DOWNLOAD_URL..."
curl -L -o "$INSTALL_DIR/$BINARY_NAME" "$DOWNLOAD_URL"

# Make executable
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo "Installed to $INSTALL_DIR/$BINARY_NAME"

# Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "Warning: $INSTALL_DIR is not in your PATH."
    echo "Add the following to your shell config (.bashrc, .zshrc):"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
fi

echo "Installation complete!"
echo "Run 'screenshot-mcp --version' to verify."
