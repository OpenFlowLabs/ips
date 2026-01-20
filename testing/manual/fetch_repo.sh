#!/usr/bin/env bash

# Script to fetch a package repository copy using pkgrecv.
# This script is intended to be run inside an OpenIndiana or OmniOS VM.

set -e

SOURCE_URL="${1:-https://pkg.openindiana.org/hipster}"
DEST_DIR="${2:-/var/share/pkg_repo}"
# Default to a small set of packages for testing if none specified
# 'entire' or '*' can be used to fetch more/all packages, but be aware of size.
shift 2 || true
PACKAGES=("$@")

if [ ${#PACKAGES[@]} -eq 0 ]; then
    echo "No packages specified, fetching a small set for testing..."
    PACKAGES=("library/zlib" "system/library")
fi

echo "Source: $SOURCE_URL"
echo "Destination: $DEST_DIR"
echo "Packages: ${PACKAGES[*]}"

if [ ! -d "$DEST_DIR" ]; then
    echo "Creating repository at $DEST_DIR..."
    mkdir -p "$DEST_DIR"
    pkgrepo create "$DEST_DIR"
    # We'll set a generic prefix, or use the one from source if we wanted to be more fancy
    pkgrepo set -s "$DEST_DIR" publisher/prefix=openindiana.org
fi

pkgrecv -s "$SOURCE_URL" -d "$DEST_DIR" "${PACKAGES[@]}" --newest


echo "Repository fetch complete."
echo "You can now sync $DEST_DIR to your host to use with pkg6depotd."
