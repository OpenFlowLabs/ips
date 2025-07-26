# GitHub Actions Workflows for IPS

This directory contains GitHub Actions workflows for the Image Packaging System (IPS) project.

## Rust CI Workflow

The `rust.yml` workflow is the main CI pipeline for the IPS project. It uses the xtask build system to build, test, and validate the codebase.

### Workflow Structure

The workflow consists of several jobs that run in sequence:

1. **Format**: Checks that the code follows Rust formatting standards using `rustfmt`.
2. **Clippy**: Runs the Rust linter to check for common mistakes and enforce code quality.
3. **Build**: Builds the project using the xtask build system.
4. **Test**: Runs unit tests for all crates.
5. **End-to-End Tests**: Builds the binaries for end-to-end tests and runs them.
6. **Documentation**: Builds the Rust documentation for the project.

### Xtask Integration

The workflow uses the xtask build system for most operations. Xtask is a Rust-based build system that allows us to write build scripts and automation tasks in Rust instead of shell scripts, making them more maintainable and cross-platform.

The following xtask commands are used in the workflow:

- `cargo run -p xtask -- build`: Builds the project
- `cargo run -p xtask -- build -r`: Builds the project in release mode
- `cargo run -p xtask -- test`: Runs unit tests
- `cargo run -p xtask -- build-e2e`: Builds binaries for end-to-end tests
- `cargo run -p xtask -- run-e2e`: Runs end-to-end tests

For more information about xtask, see the [xtask README](../xtask/README.md).

### Best Practices

The workflow follows several best practices for GitHub Actions:

1. **Caching**: Dependencies are cached to speed up builds.
2. **Matrix Strategy**: The build and test jobs use a matrix strategy to allow testing on multiple platforms and Rust versions.
3. **Job Dependencies**: Jobs are properly sequenced to ensure efficient execution.
4. **Artifact Uploads**: Build artifacts and documentation are uploaded for later use.
5. **Error Handling**: Warnings are treated as errors to maintain code quality.
6. **Manual Triggering**: The workflow can be triggered manually using the workflow_dispatch event.

### Running the Workflow Locally

You can run the same checks locally using the following commands:

```bash
# Format check
cargo fmt --all -- --check

# Clippy
cargo clippy --all-targets --all-features -- -D warnings

# Build
cargo run -p xtask -- build

# Test
cargo run -p xtask -- test

# End-to-End Tests
cargo run -p xtask -- build-e2e
cargo run -p xtask -- run-e2e

# Documentation
cargo doc --no-deps
```

## Troubleshooting

If you encounter issues with the workflow:

1. Check that your code passes all checks locally.
2. Ensure that the xtask crate is properly set up.
3. Look at the workflow logs for specific error messages.
4. For end-to-end test failures, try running the tests locally with more verbose output.