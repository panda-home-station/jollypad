#!/bin/bash
set -e

echo "🚀 Launching JollyPad via Rust Launcher..."

cargo build --workspace
# Run the Rust launcher
# This assumes the project has been built with `cargo build`
echo "Check ~/jolly-home.log for UI logs"
cargo run -p jolly-launcher --bin jolly-launcher
