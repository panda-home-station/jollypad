#!/bin/bash
# Auto-restored start script for JollyPad

# Ensure we are in the project directory
cd /home/jolly/phs/jollypad

# Redirect all output (stdout and stderr) to jollypad.log
exec > jollypad.log 2>&1

# Set logging level
export RUST_LOG=info

# Try to run the release binary of the launcher
if [ -f "./target/release/jolly-launcher" ]; then
    exec ./target/release/jolly-launcher
else
    # Fallback to cargo run if binary doesn't exist
    echo "Binary not found, building and running..."
    exec cargo run --release --bin jolly-launcher
fi
