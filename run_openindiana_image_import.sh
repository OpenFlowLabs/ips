#!/usr/bin/env bash

# Create an image under sample_data/test-image and import OpenIndiana catalogs
# so you can inspect the results locally.
#
# Usage:
#   ./run_openindiana_image_import.sh
#
# Notes:
# - Requires network access to https://pkg.openindiana.org/hipster
# - You can change RUST_LOG below to control verbosity (error|warn|info|debug|trace)

set -euo pipefail
set -x

export RUST_LOG=info

IMG_PATH="sample_data/test-image"
PUBLISHER="openindiana.org"
ORIGIN="https://pkg.openindiana.org/hipster"
PKG6_BIN="./target/debug/pkg6"

# Ensure sample_data exists and reset image dir for a clean run
mkdir -p "$(dirname "$IMG_PATH")"
if [ -d "$IMG_PATH" ]; then
  rm -rf "$IMG_PATH"
fi

# Build pkg6 (and dependencies)
cargo build -p pkg6

# 1) Create image and add publisher (this also downloads the per-publisher catalog files)
"$PKG6_BIN" image-create \
  -F "$IMG_PATH" \
  -p "$PUBLISHER" \
  -g "$ORIGIN"

# 2) Build the merged image-wide catalog database (also refreshes per-publisher catalogs)
"$PKG6_BIN" -R "$IMG_PATH" refresh "$PUBLISHER"

# 3) Print database statistics so you can inspect counts quickly
"$PKG6_BIN" -R "$IMG_PATH" debug-db --stats

# Optional: show configured publishers
"$PKG6_BIN" -R "$IMG_PATH" publisher -o table

echo "Done. Image created at: $IMG_PATH"
echo "Per-publisher catalog files under: $IMG_PATH/var/pkg/catalog/$PUBLISHER"
echo "Merged catalog database at: $IMG_PATH/var/pkg/catalog.redb"
