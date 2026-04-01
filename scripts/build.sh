#!/bin/bash
set -e

echo "Building release binary..."
cargo build --release

echo "Compressing binary with UPX..."
upx -9 target/release/ns

echo "Done! Binary is ready at target/release/ns"
