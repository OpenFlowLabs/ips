#!/usr/bin/env bash

# Script to build and run pkg6depotd for manual testing.

set -e

REPO_ROOT="${1:-/tmp/pkg_repo}"

echo "Building pkg6depotd..."
cargo build -p pkg6depotd

if [ ! -d "$REPO_ROOT" ]; then
    echo "Warning: Repository root $REPO_ROOT does not exist."
    echo "You might want to fetch a repository first using fetch_repo.sh inside a VM"
    echo "and then sync it to this path."
fi

# Create a temporary config file based on the one in the root but with the correct repo path
CONFIG_FILE="/tmp/pkg6depotd_test.kdl"
cat > "$CONFIG_FILE" <<EOF
server {
    bind "0.0.0.0:8080"
    workers 4
}

repository {
    root "$REPO_ROOT"
    mode "readonly"
}

telemetry {
    service-name "pkg6depotd"
    log-format "json"
}
EOF

echo "Starting pkg6depotd with config $CONFIG_FILE..."
RUST_LOG=debug ./target/debug/pkg6depotd -c "$CONFIG_FILE" start
