#!/bin/bash

# Test script to verify that repository operations work with the new directory structure

# Create a temporary directory for the test
TEST_DIR="/tmp/pkg6_repo_operations_test"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"

echo "Created test directory: $TEST_DIR"

# Create a repository
echo "Creating repository..."
cargo run --bin pkg6repo -- create "$TEST_DIR/repo"

# Add a publisher
echo "Adding publisher..."
cargo run --bin pkg6repo -- add-publisher -r "$TEST_DIR/repo" test

# Create a test file
echo "Creating test file..."
echo "This is a test file" > "$TEST_DIR/test_file.txt"

# Create a simple manifest
echo "Creating manifest..."
cat > "$TEST_DIR/test.p5m" << EOF
{
  "attributes": [
    {
      "key": "pkg.fmri",
      "values": [
        "pkg://test/example@1.0,5.11-0:20250727T123000Z"
      ],
      "properties": {}
    }
  ],
  "files": [
    {
      "path": "usr/share/doc/example/test.txt",
      "mode": "0644",
      "owner": "root",
      "group": "root"
    }
  ]
}
EOF

# Create a prototype directory
echo "Creating prototype directory..."
mkdir -p "$TEST_DIR/prototype/usr/share/doc/example"
cp "$TEST_DIR/test_file.txt" "$TEST_DIR/prototype/usr/share/doc/example/test.txt"

# Publish the package
echo "Publishing package..."
cargo run --bin pkg6repo -- publish -r "$TEST_DIR/repo" -p test -m "$TEST_DIR/test.p5m" "$TEST_DIR/prototype"

# Check if the file was stored in the correct directory structure
echo "Checking file structure..."
find "$TEST_DIR/repo/file" -type f | while read -r file; do
    hash=$(basename "$file")
    first_two=${hash:0:2}
    next_two=${hash:2:2}
    expected_path="$TEST_DIR/repo/file/$first_two/$next_two/$hash"
    
    if [ "$file" = "$expected_path" ]; then
        echo "SUCCESS: File was stored at the correct path: $file"
    else
        echo "ERROR: File was stored at an unexpected path: $file"
        echo "Expected: $expected_path"
    fi
done

# Clean up
echo "Cleaning up..."
rm -rf "$TEST_DIR"

echo "Test completed."