# Error Handling Guidelines for IPS Rust Code

This document outlines best practices for error handling in the IPS Rust codebase. It covers how to use the `miette` and `thiserror` crates for robust error handling and reporting, and the `tracing` crate for configurable debug output.

## Core Principles

The core idea is to combine:
- `thiserror` for creating custom error types with clear error messages
- `miette` for rich, user-friendly error reporting with diagnostic information
- `tracing` for structured, configurable debug output

## Project Setup

### Dependencies

Add the necessary dependencies to your crate's `Cargo.toml`:

```toml
[dependencies]
miette = { version = "7.6.0", features = ["fancy"] }
thiserror = "1.0.50"
tracing = "0.1.37"
tracing-subscriber = "0.3.17"
```

**Note**: The "fancy" feature for miette enables colorful, detailed error reports. This feature should only be enabled in the top-level crate of your project (application crates) to avoid unnecessary dependencies in library crates.

For library crates like `libips`, use:

```toml
[dependencies]
miette = "7.6.0"
thiserror = "1.0.50"
tracing = "0.1.37"
```

## Defining Custom Error Types

Define your error types as enums using `thiserror`. This allows you to create specific error variants for different failure scenarios. Then, use miette's `Diagnostic` derive macro to add rich diagnostic information to your errors.

### Example

```rust
use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
#[error("A validation error occurred")]
#[diagnostic(
    code(ips::validation_error),
    help("Please check the input data and try again.")
)]
pub enum ValidationError {
    #[error("Invalid package name: {0}")]
    #[diagnostic(
        code(ips::validation_error::invalid_name),
        help("Package names must follow the IPS naming conventions.")
    )]
    InvalidName(String),

    #[error("Invalid version format")]
    #[diagnostic(
        code(ips::validation_error::invalid_version),
        help("Version must be in the format: major.minor.patch")
    )]
    InvalidVersion {
        #[source_code]
        src: NamedSource,

        #[label("the invalid version")]
        span: SourceSpan,
    },
}
```

In this example:

- `#[derive(Error, Debug, Diagnostic)]` automatically implements the necessary traits from thiserror and miette.
- `#[error("...")]` provides the main error message.
- `#[diagnostic(...)]` adds diagnostic information like a unique error code and a help message.
- For more specific errors, like `InvalidVersion`, you can provide `source_code` and a `label` with a `SourceSpan` to highlight the exact location of the error.

### Error Codes

Use a consistent naming scheme for error codes:

- Top-level errors: `crate_name::error_category`
- Specific errors: `crate_name::error_category::specific_error`

For example:
- `ips::validation_error`
- `ips::validation_error::invalid_name`

## Returning Errors

### In Library Code

In library code (like `libips`), always return your specific error types. This makes your library's API clear and easy to use.

```rust
pub fn validate_package(name: &str, version: &str) -> Result<(), ValidationError> {
    if !is_valid_package_name(name) {
        return Err(ValidationError::InvalidName(name.to_string()));
    }

    if !is_valid_version(version) {
        let source = NamedSource::new("input.txt", version.to_string());
        let span = (0, version.len()).into();
        return Err(ValidationError::InvalidVersion { src: source, span });
    }

    Ok(())
}
```

### In Application Code

In your application's main function and other top-level code, you can use `miette::Result` for convenience.

```rust
use miette::{IntoDiagnostic, Result};

fn main() -> Result<()> {
    // Initialize tracing
    setup_tracing();

    // Your application logic here
    let config = std::fs::read_to_string("config.json").into_diagnostic()?;
    
    // Process the config
    process_config(&config).map_err(|e| e.into())?;
    
    Ok(())
}
```

### Converting Between Error Types

To convert from one error type to another, you can use the `From` trait or the `map_err` method on `Result`.

```rust
// Using From trait
impl From<std::io::Error> for MyError {
    fn from(err: std::io::Error) -> Self {
        MyError::IoError(err)
    }
}

// Using map_err
fn read_config() -> Result<Config, MyError> {
    std::fs::read_to_string("config.json")
        .map_err(|e| MyError::IoError(e))?
        .parse()
        .map_err(|e| MyError::ParseError(e))
}
```

## Using Miette's Diagnostic Features

Miette provides several features to enhance error reporting:

### Source Code Highlighting

You can include the source code that caused the error and highlight the specific part that's problematic:

```rust
#[error("Invalid syntax in configuration file")]
struct InvalidSyntax {
    #[source_code]
    src: NamedSource,

    #[label("this syntax is invalid")]
    span: SourceSpan,
}
```

To use this in your code:

```rust
let source = NamedSource::new("config.json", config_content.to_string());
let span = (error_start, error_length).into();
return Err(ConfigError::InvalidSyntax { src: source, span });
```

### Related Information

You can add related information to provide context for the error:

```rust
#[error("Failed to process package")]
#[diagnostic(
    code(ips::package_error),
    help("Check the package manifest for errors.")
)]
ProcessError {
    #[related]
    related: Vec<ValidationError>,
}
```

### Custom Help Messages

Provide helpful messages to guide users on how to fix the error:

```rust
#[error("Invalid configuration")]
#[diagnostic(
    code(ips::config_error),
    help("The configuration file must be valid JSON. Check the syntax and try again.")
)]
InvalidConfig,
```

## Configurable Debug Output with Tracing

The `tracing` crate provides a powerful framework for structured, event-based logging. You can use it to add debug output to your library that can be enabled or disabled at runtime.

### Setting Up Tracing

In your application's main function, initialize the tracing-subscriber:

```rust
use tracing_subscriber::{EnvFilter, FmtSubscriber};

fn setup_tracing() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");
}
```

### Instrumenting Your Code

Use the tracing macros (`trace!`, `debug!`, `info!`, `warn!`, `error!`) to add log statements to your code. You can also use the `#[tracing::instrument]` attribute to automatically log the entry and exit of a function, along with its arguments.

```rust
#[tracing::instrument]
pub fn process_package(package: &Package) -> Result<(), ProcessError> {
    tracing::debug!("Processing package: {}", package.name);
    
    // Your logic here
    
    tracing::info!("Package processed successfully");
    Ok(())
}
```

### Controlling Log Levels

You can control the log level using the `RUST_LOG` environment variable. For example, to enable debug output, you would run your application like this:

```bash
RUST_LOG=debug cargo run
```

Common log levels, from most to least verbose:
- `trace`: Very detailed information, typically only useful for debugging specific issues
- `debug`: Useful information for debugging
- `info`: General information about the application's operation
- `warn`: Potentially problematic situations that don't prevent the application from working
- `error`: Error conditions that prevent some functionality from working

## Best Practices

1. **Be Specific**: Create specific error variants for different failure scenarios.
2. **Be Helpful**: Include helpful error messages and diagnostic information.
3. **Be Consistent**: Use a consistent naming scheme for error codes.
4. **Be Transparent**: Use `#[error(transparent)]` for wrapping errors from dependencies.
5. **Be Traceable**: Use tracing to log important events and debug information.

## Example: Converting Existing Code

Here's an example of how to convert an existing error type to use miette:

### Before

```rust
#[derive(Debug, Error)]
pub enum FmriError {
    #[error("invalid FMRI format")]
    InvalidFormat,
    #[error("invalid version format")]
    InvalidVersionFormat,
    #[error("invalid release format")]
    InvalidReleaseFormat,
}
```

### After

```rust
#[derive(Debug, Error, Diagnostic)]
#[diagnostic(code(ips::fmri_error))]
pub enum FmriError {
    #[error("invalid FMRI format")]
    #[diagnostic(
        help("FMRI must be in the format: pkg://publisher/package@version")
    )]
    InvalidFormat,
    
    #[error("invalid version format")]
    #[diagnostic(
        help("Version must be in the format: major.minor.patch")
    )]
    InvalidVersionFormat,
    
    #[error("invalid release format")]
    #[diagnostic(
        help("Release must be a dot-separated list of numbers")
    )]
    InvalidReleaseFormat,
}
```

## Further Reading

- [miette documentation](https://docs.rs/miette/7.6.0/miette/)
- [thiserror documentation](https://docs.rs/thiserror/1.0.50/thiserror/)
- [tracing documentation](https://docs.rs/tracing/0.1.37/tracing/)