#!/bin/bash

set -e
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_DIR"

MODE="${1:-release}"

if [[ "$MODE" == "release" ]]; then
    echo "Building in release mode..."
    cargo build --release
    SRC="target/release/lumino-rs"
elif [[ "$MODE" == "debug" ]]; then
    echo "Building in debug mode..."
    cargo build
    SRC="target/debug/lumino-rs"
else
    echo "Invalid mode. Usage: $0 [release|debug]"
    echo "Default is release mode."
    exit 1
fi

mkdir -p bin

cp "$SRC" "bin/lumino-rs"

echo "Build completed successfully!"
echo "Executable copied to: bin/lumino-rs"
