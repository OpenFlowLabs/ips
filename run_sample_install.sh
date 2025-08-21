#!/usr/bin/env bash

# Run a sample installation into sample_data/test-image with dry-run and real run.
#
# This script will:
#  1) Build the pkg6 CLI
#  2) Create or reset a test image at sample_data/test-image
#  3) Configure the openindiana.org publisher
#     - If sample_data/pkg6-repo exists, use it via file:// origin
#     - Otherwise, use the OpenIndiana network origin (requires internet)
#  4) Refresh catalogs for the image
#  5) Install a package first with dry-run, then for real
#
# Usage:
#   ./run_sample_install.sh [PKG_NAME]
# Environment variables:
#   PKG_NAME   Package stem/FMRI pattern to install (default: database/postgres/connector/jdbc)
#   RUST_LOG   Rust log level (default: info)
#
# Notes:
#  - The current installer writes empty files as payloads (scaffold). It does create dirs/links.
#  - All file system operations are performed relative to the image root (sample_data/test-image).
#  - If you need to seed a local sample repo, see: ./run_local_import_test.sh

set -euo pipefail
set -x

export RUST_LOG="${RUST_LOG:-info}"

IMG_PATH="sample_data/test-image"
PUBLISHER="openindiana.org"
LOCAL_REPO_DIR="sample_data/pkg6-repo"
PKG6_BIN="./target/debug/pkg6"

# Package to install
PKG_NAME="${1:-${PKG_NAME:-database/postgres/connector/jdbc}}"

# Determine origin: use local file repo if present, otherwise network origin
if [ -d "$LOCAL_REPO_DIR" ]; then
  ORIGIN="file://$(pwd)/$LOCAL_REPO_DIR"
else
  ORIGIN="https://pkg.openindiana.org/hipster"
fi

echo "Using origin: $ORIGIN"

echo "Building pkg6 (debug)"
cargo build -p pkg6

# Prepare image path
mkdir -p "$(dirname "$IMG_PATH")"
if [ -d "$IMG_PATH" ]; then
  rm -rf "$IMG_PATH"
fi

# 1) Create image and add publisher
"$PKG6_BIN" image-create \
  -F "$IMG_PATH" \
  -p "$PUBLISHER" \
  -g "$ORIGIN"

# 2) Refresh catalogs (also downloads per-publisher catalogs)
"$PKG6_BIN" -R "$IMG_PATH" refresh "$PUBLISHER"

# 3) Show publishers for confirmation (table output)
"$PKG6_BIN" -R "$IMG_PATH" publisher -o table

# 4) Real install
RUST_LOG=debug "$PKG6_BIN" -R "$IMG_PATH" install "pkg://$PUBLISHER/$PKG_NAME" || {
  echo "Real install failed" >&2
  exit 1
}

# 5) Show installed packages
"$PKG6_BIN" -R "$IMG_PATH" list

# 6) Dump installed database
"$PKG6_BIN" -R "$IMG_PATH" debug-db --dump-table installed

echo "Sample installation completed successfully at $IMG_PATH"
