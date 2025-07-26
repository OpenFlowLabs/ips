# Repository Testing

This document describes the testing infrastructure for the repository implementation in the IPS project.

## Overview

The repository implementation is tested at two levels:

1. **Unit Tests**: Tests for the individual components of the repository implementation, such as the FileBackend, CatalogManager, etc.
2. **End-to-End Tests**: Tests for the complete workflow of creating a repository, adding packages, and querying the repository.

## Test Setup

Before running the tests, you need to set up the test environment by running the `setup_test_env.sh` script:

```bash
./setup_test_env.sh
```

This script:
1. Compiles the application
2. Creates a prototype directory with example files
3. Creates package manifests for testing

## Unit Tests

The unit tests are implemented in `libips/src/repository/tests.rs`. These tests cover:

- Creating a repository
- Adding a publisher
- Testing the CatalogManager functionality
- Publishing files to a repository
- Listing packages in a repository
- Showing package contents
- Searching for packages

To run the unit tests:

```bash
cargo test repository::tests
```

**Note**: Some of the unit tests are currently failing due to issues with how packages are created and queried. These issues need to be addressed in future work.

## End-to-End Tests

The end-to-end tests are implemented in `pkg6repo/src/e2e_tests.rs`. These tests use the actual command-line tools to test the complete workflow:

- Creating a repository using pkg6repo
- Adding a publisher to a repository
- Publishing a package to a repository using pkg6dev
- Showing package contents
- Publishing multiple packages

To run the end-to-end tests:

```bash
cargo test -p pkg6repo
```

**Note**: The end-to-end tests are currently failing due to a conflict with the argument name 'version' in the pkg6repo command-line interface. This issue needs to be addressed in future work.

## Future Work

1. Fix the unit tests to properly create and query packages
2. Fix the conflict with the argument name 'version' in the pkg6repo command-line interface
3. Add more comprehensive tests for edge cases and error conditions
4. Add tests for the RestBackend implementation
5. Add tests for the repository search functionality