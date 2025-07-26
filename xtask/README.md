# Xtask for IPS

This directory contains the xtask implementation for the IPS project. Xtask is a Rust-based build system that allows us to write build scripts and automation tasks in Rust instead of shell scripts, making them more maintainable and cross-platform.

## Available Commands

The following commands are available through cargo-xtask:

```bash
cargo run -p xtask -- setup-test-env    # Set up the test environment for repository tests
cargo run -p xtask -- build             # Build the project
cargo run -p xtask -- build -r          # Build with release optimizations
cargo run -p xtask -- build -p <crate>  # Build a specific crate
cargo run -p xtask -- test              # Run tests
cargo run -p xtask -- test -r           # Run tests with release optimizations
cargo run -p xtask -- test -p <crate>   # Run tests for a specific crate
cargo run -p xtask -- build-e2e         # Build binaries for end-to-end tests
cargo run -p xtask -- run-e2e           # Run end-to-end tests using pre-built binaries
cargo run -p xtask -- run-e2e -t <test> # Run a specific end-to-end test
cargo run -p xtask -- fmt               # Format code using cargo fmt
cargo run -p xtask -- clippy            # Run clippy for code quality checks
cargo run -p xtask -- clean             # Clean build artifacts
```

## End-to-End Testing

End-to-end tests are an important part of the IPS project. They test the entire system from the user's perspective, ensuring that all components work together correctly.

### Improved End-to-End Testing Approach

To reduce flaky tests and improve reliability, we've separated the building of binaries from the test execution. This approach has several advantages:

1. **Reduced Flakiness**: By separating the build step from the test execution, we reduce the chance of tests failing due to build issues.
2. **Faster Test Execution**: Pre-building the binaries means that tests can start immediately without waiting for compilation.
3. **Consistent Test Environment**: Using xtask for both building binaries and setting up the test environment ensures consistency.

### How to Run End-to-End Tests

To run end-to-end tests, follow these steps:

1. Build the binaries for end-to-end tests:
   ```bash
   cargo run -p xtask -- build-e2e
   ```

2. Run the end-to-end tests:
   ```bash
   cargo run -p xtask -- run-e2e
   ```

   To run a specific test:
   ```bash
   cargo run -p xtask -- run-e2e -t test_e2e_create_repository
   ```

### How It Works

The `build-e2e` command:
- Builds the necessary binaries (pkg6repo, pkg6dev, etc.) in release mode
- Copies the binaries to a dedicated directory (`/tmp/pkg6_test/bin`)

The `run-e2e` command:
- Checks if the pre-built binaries exist, and builds them if they don't
- Sets up the test environment using the `setup-test-env` command
- Runs the end-to-end tests, passing the location of the pre-built binaries via an environment variable

The end-to-end tests:
- Use the pre-built binaries instead of compiling them on the fly
- Use a consistent test environment setup

## Adding New Commands

To add a new command to cargo-xtask:

1. Edit the `xtask/src/main.rs` file
2. Add a new variant to the `Commands` enum
3. Implement a function for the new command
4. Add a match arm in the `main` function to call your new function