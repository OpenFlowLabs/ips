#!/bin/bash
# Script to set up the test environment for repository tests

set -e  # Exit on error

# Directory where test files will be created
TEST_BASE_DIR="/tmp/pkg6_test"
PROTOTYPE_DIR="$TEST_BASE_DIR/prototype"
MANIFEST_DIR="$TEST_BASE_DIR/manifests"

# Clean up any existing test directories
if [ -d "$TEST_BASE_DIR" ]; then
    echo "Cleaning up existing test directory..."
    rm -rf "$TEST_BASE_DIR"
fi

# Create test directories
echo "Creating test directories..."
mkdir -p "$PROTOTYPE_DIR"
mkdir -p "$MANIFEST_DIR"

# Compile the applications
echo "Compiling applications..."
cd "$(dirname "$0")"
cargo build

# Create a simple prototype directory structure with some files
echo "Creating prototype directory structure..."

# Create some directories
mkdir -p "$PROTOTYPE_DIR/usr/bin"
mkdir -p "$PROTOTYPE_DIR/usr/share/doc/example"
mkdir -p "$PROTOTYPE_DIR/etc/config"

# Create some files
echo "#!/bin/sh\necho 'Hello, World!'" > "$PROTOTYPE_DIR/usr/bin/hello"
chmod +x "$PROTOTYPE_DIR/usr/bin/hello"

echo "This is an example document." > "$PROTOTYPE_DIR/usr/share/doc/example/README.txt"

echo "# Example configuration file\nvalue=42" > "$PROTOTYPE_DIR/etc/config/example.conf"

# Create a simple manifest
echo "Creating package manifest..."
cat > "$MANIFEST_DIR/example.p5m" << EOF
set name=pkg.fmri value=pkg://test/example@1.0.0
set name=pkg.summary value="Example package for testing"
set name=pkg.description value="This is an example package used for testing the repository implementation."
set name=info.classification value="org.opensolaris.category.2008:System/Core"
set name=variant.arch value=i386 value=sparc
file path=usr/bin/hello mode=0755 owner=root group=bin
file path=usr/share/doc/example/README.txt mode=0644 owner=root group=bin
file path=etc/config/example.conf mode=0644 owner=root group=bin preserve=true
dir path=usr/bin mode=0755 owner=root group=bin
dir path=usr/share/doc/example mode=0755 owner=root group=bin
dir path=etc/config mode=0755 owner=root group=sys
EOF

# Create a second manifest for testing multiple packages
cat > "$MANIFEST_DIR/example2.p5m" << EOF
set name=pkg.fmri value=pkg://test/example2@1.0.0
set name=pkg.summary value="Second example package for testing"
set name=pkg.description value="This is a second example package used for testing the repository implementation."
set name=info.classification value="org.opensolaris.category.2008:System/Core"
set name=variant.arch value=i386 value=sparc
file path=usr/bin/hello mode=0755 owner=root group=bin
file path=usr/share/doc/example/README.txt mode=0644 owner=root group=bin
dir path=usr/bin mode=0755 owner=root group=bin
dir path=usr/share/doc/example mode=0755 owner=root group=bin
EOF

echo "Test environment setup complete!"
echo "Prototype directory: $PROTOTYPE_DIR"
echo "Manifest directory: $MANIFEST_DIR"