#!/usr/bin/env bash
set -euo pipefail

# Build script: builds Rust staticlib and the Qt6 client.
# Usage: ./build-and-run.sh [--no-run]

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT_DIR=$(cd "${SCRIPT_DIR}/.." && pwd)

echo "Building Rust fsearch-core (release)..."
cd "${ROOT_DIR}"
cargo build -p fsearch-core --release

echo "Configuring and building Qt6 client..."
cd "${SCRIPT_DIR}"
mkdir -p build
cd build

# Try to find Qt6 via environment, otherwise user must pass -DCMAKE_PREFIX_PATH
cmake .. -DFSEARCH_CORE_LIB="${ROOT_DIR}/target/release/libfsearch_core.a" "$@"
cmake --build .

if [ "${1-}" != "--no-run" ]; then
    echo "Running fsearch_qt_client..."
    ./fsearch_qt_client
fi
