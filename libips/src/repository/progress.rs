//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use std::fmt;

/// Trait for reporting progress during long-running operations like catalog downloads.
///
/// Implementors of this trait can be passed to methods that support progress reporting,
/// such as `download_catalog` in the `RestBackend`. This allows for flexible progress
/// reporting in different UI contexts (CLI, GUI, etc.).
///
/// # Examples
///
/// ```
/// use libips::repository::progress::{ProgressReporter, ProgressInfo};
///
/// struct SimpleProgressReporter;
///
/// impl ProgressReporter for SimpleProgressReporter {
///     fn start(&self, info: &ProgressInfo) {
///         println!("Starting: {}", info.operation);
///     }
///
///     fn update(&self, info: &ProgressInfo) {
///         if let (Some(current), Some(total)) = (info.current, info.total) {
///             let percentage = (current as f64 / total as f64) * 100.0;
///             println!("{}: {:.1}% ({}/{})", info.operation, percentage, current, total);
///         }
///     }
///
///     fn finish(&self, info: &ProgressInfo) {
///         println!("Finished: {}", info.operation);
///     }
/// }
/// ```
pub trait ProgressReporter {
    /// Called when an operation starts.
    ///
    /// # Arguments
    ///
    /// * `info` - Information about the operation
    fn start(&self, info: &ProgressInfo);

    /// Called when progress is made during an operation.
    ///
    /// # Arguments
    ///
    /// * `info` - Information about the operation and current progress
    fn update(&self, info: &ProgressInfo);

    /// Called when an operation completes.
    ///
    /// # Arguments
    ///
    /// * `info` - Information about the completed operation
    fn finish(&self, info: &ProgressInfo);
}

/// Information about a progress-reporting operation.
#[derive(Debug, Clone)]
pub struct ProgressInfo {
    /// The name of the operation being performed
    pub operation: String,

    /// The current progress value (e.g., bytes downloaded, files processed)
    pub current: Option<u64>,

    /// The total expected value (e.g., total bytes, total files)
    pub total: Option<u64>,

    /// Additional context about the operation (e.g., current file name)
    pub context: Option<String>,
}

impl ProgressInfo {
    /// Create a new ProgressInfo for an operation.
    ///
    /// # Arguments
    ///
    /// * `operation` - The name of the operation
    ///
    /// # Returns
    ///
    /// A new ProgressInfo with only the operation name set
    pub fn new(operation: impl Into<String>) -> Self {
        ProgressInfo {
            operation: operation.into(),
            current: None,
            total: None,
            context: None,
        }
    }

    /// Set the current progress value.
    ///
    /// # Arguments
    ///
    /// * `current` - The current progress value
    ///
    /// # Returns
    ///
    /// Self for method chaining
    pub fn with_current(mut self, current: u64) -> Self {
        self.current = Some(current);
        self
    }

    /// Set the total expected value.
    ///
    /// # Arguments
    ///
    /// * `total` - The total expected value
    ///
    /// # Returns
    ///
    /// Self for method chaining
    pub fn with_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }

    /// Set additional context about the operation.
    ///
    /// # Arguments
    ///
    /// * `context` - Additional context (e.g., current file name)
    ///
    /// # Returns
    ///
    /// Self for method chaining
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

impl fmt::Display for ProgressInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.operation)?;

        if let (Some(current), Some(total)) = (self.current, self.total) {
            let percentage = (current as f64 / total as f64) * 100.0;
            write!(f, " {:.1}% ({}/{})", percentage, current, total)?;
        } else if let Some(current) = self.current {
            write!(f, " {}", current)?;
        }

        if let Some(context) = &self.context {
            write!(f, " - {}", context)?;
        }

        Ok(())
    }
}

/// A no-op implementation of ProgressReporter that does nothing.
///
/// This is useful as a default when progress reporting is not needed.
#[derive(Debug, Clone, Copy)]
pub struct NoopProgressReporter;

impl ProgressReporter for NoopProgressReporter {
    fn start(&self, _info: &ProgressInfo) {}
    fn update(&self, _info: &ProgressInfo) {}
    fn finish(&self, _info: &ProgressInfo) {}
}
