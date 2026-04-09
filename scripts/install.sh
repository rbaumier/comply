#!/usr/bin/env bash
set -euo pipefail

echo "comply: building release binary..."
cargo build --release

INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "${INSTALL_DIR}"
cp target/release/comply "${INSTALL_DIR}/comply"

echo "comply installed to ${INSTALL_DIR}/comply"
echo "Make sure ${INSTALL_DIR} is in your PATH."
