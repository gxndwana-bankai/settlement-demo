#!/bin/bash

set -e

echo "🚀 Building Settlement CLI..."
echo

cd script
cargo build --release --bin cli

echo
echo "✅ Build complete!"
echo
echo "The CLI binary is located at:"
echo "  $(pwd)/../target/release/cli"
echo
echo "You can run it directly:"
echo "  ../target/release/cli --help"
echo
echo "Or create an alias by adding this to your ~/.zshrc or ~/.bashrc:"
echo "  alias settlement-cli='$(pwd)/../target/release/cli'"
echo
echo "Or create a symlink (requires sudo):"
echo "  sudo ln -sf $(pwd)/../target/release/cli /usr/local/bin/settlement-cli"
echo

