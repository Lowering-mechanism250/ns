#!/bin/bash
set -e

# Version to download if no local build is found
VERSION="v0.1.0"
BIN_NAME="ns"
INSTALL_DIR="/usr/local/bin"

if [ "$EUID" -ne 0 ]; then
    echo "This script requires root privileges to install the binary to $INSTALL_DIR."
    echo "Please run using: sudo $0"
    exit 1
fi

if [ -f "target/release/$BIN_NAME" ]; then
    echo "Local build found: target/release/$BIN_NAME"
    echo "Installing to $INSTALL_DIR/$BIN_NAME..."
    cp "target/release/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
    chmod +x "$INSTALL_DIR/$BIN_NAME"
    echo "Installation successful!"

elif [ -d "src" ] && [ -f "Cargo.toml" ]; then
    echo "Error: Source files found but the binary was not compiled yet."
    echo "Please run './scripts/build.sh' (or 'cargo build --release') to build before installing."
    exit 1

else
    echo "No local build or source code found. Downloading pre-compiled binary..."
    URL="https://github.com/sammwyy/ns/releases/download/${VERSION}/ns-linux-amd64"
    echo "Fetching: $URL"
    
    # Download to a temporary location first
    TMP_BIN="/tmp/$BIN_NAME"
    
    # Using curl and falling back to wget if not available
    if command -v curl >/dev/null 2>&1; then
        curl -fLo "$TMP_BIN" "$URL" || { echo "Download failed. Please check the version/tag or URL."; exit 1; }
    elif command -v wget >/dev/null 2>&1; then
        wget -O "$TMP_BIN" "$URL" || { echo "Download failed. Please check the version/tag or URL."; exit 1; }
    else
        echo "Error: Neither curl nor wget is installed. Cannot download the binary."
        exit 1
    fi
    
    echo "Installing to $INSTALL_DIR/$BIN_NAME..."
    mv "$TMP_BIN" "$INSTALL_DIR/$BIN_NAME"
    chmod +x "$INSTALL_DIR/$BIN_NAME"
    echo "Installation successful!"
fi
