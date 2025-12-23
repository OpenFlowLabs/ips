// This Source Code Form is subject to the terms of
// the Mozilla Public License, v. 2.0. If a copy of the
// MPL was not distributed with this file, You can
// obtain one at https://mozilla.org/MPL/2.0/.

//! FMRI (Fault Management Resource Identifier) implementation
//!
//! An FMRI is a unique identifier for a package in the IPS system.
//! It follows the format: pkg://publisher/package_name@version
//! where:
//! - publisher is optional
//! - version is optional and follows the format: release[,branch][-build]:timestamp
//!   - release is a dot-separated vector of digits (e.g., 5.11)
//!   - branch is optional and is a dot-separated vector of digits (e.g., 1)
//!   - build is optional and is a dot-separated vector of digits (e.g., 2020.0.1.0)
//!   - timestamp is optional and is a hexadecimal string (e.g., 20200421T195136Z)
//!
//! The dot-separated vector components (release, branch, build) can be converted to and from
//! semver::Version objects using the provided conversion methods:
//! - release_to_semver
//! - branch_to_semver
//! - build_to_semver
//! - from_semver
//!
//! Examples:
//! - pkg:///sunos/coreutils@5.11,1:[hex-timestamp-1]
//! - pkg://openindiana.org/web/server/nginx@1.18.0,5.11-2020.0.1.0:20200421T195136Z
//! - pkg:/system/library@0.5.11-2020.0.1.19563
//! - xvm@0.5.11-2015.0.2.0
//!
//! # Examples
//!
//! ```
//! use libips::fmri::{Fmri, Version};
//!
//! // Parse an FMRI
//! let fmri = Fmri::parse("pkg://openindiana.org/web/server/nginx@1.18.0,5.11-2020.0.1.0:20200421T195136Z").unwrap();
//!
//! // Convert the release component to a semver::Version
//! if let Some(version) = &fmri.version {
//!     let semver = version.release_to_semver().unwrap();
//!     assert_eq!(semver.major, 1);
//!     assert_eq!(semver.minor, 18);
//!     assert_eq!(semver.patch, 0);
//! }
//!
//! // Create a Version from semver::Version components
//! let release = semver::Version::new(5, 11, 0);
//! let branch = Some(semver::Version::new(1, 0, 0));
//! let build = Some(semver::Version::new(2020, 0, 1));
//! let timestamp = Some("20200421T195136Z".to_string());
//!
//! let version = Version::from_semver(release, branch, build, timestamp);
//! assert_eq!(version.release, "5.11.0");
//! assert_eq!(version.branch, Some("1.0.0".to_string()));
//! assert_eq!(version.build, Some("2020.0.1".to_string()));
//! assert_eq!(version.timestamp, Some("20200421T195136Z".to_string()));
//! ```

use diff::Diff;
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Errors that can occur when parsing an FMRI
#[derive(Debug, Error, Diagnostic, PartialEq)]
pub enum FmriError {
    #[error("invalid FMRI format")]
    #[diagnostic(
        code(ips::fmri_error::invalid_format),
        help("FMRI should be in the format: [scheme://][publisher/]name[@version]")
    )]
    InvalidFormat,

    #[error("invalid version format")]
    #[diagnostic(
        code(ips::fmri_error::invalid_version_format),
        help("Version should be in the format: release[,branch][-build][:timestamp]")
    )]
    InvalidVersionFormat,

    #[error("invalid release format")]
    #[diagnostic(
        code(ips::fmri_error::invalid_release_format),
        help("Release should be a dot-separated vector of digits (e.g., 5.11)")
    )]
    InvalidReleaseFormat,

    #[error("invalid branch format")]
    #[diagnostic(
        code(ips::fmri_error::invalid_branch_format),
        help("Branch should be a dot-separated vector of digits (e.g., 1)")
    )]
    InvalidBranchFormat,

    #[error("invalid build format")]
    #[diagnostic(
        code(ips::fmri_error::invalid_build_format),
        help("Build should be a dot-separated vector of digits (e.g., 2020.0.1.0)")
    )]
    InvalidBuildFormat,

    #[error("invalid timestamp format")]
    #[diagnostic(
        code(ips::fmri_error::invalid_timestamp_format),
        help("Timestamp should be a hexadecimal string (e.g., 20200421T195136Z)")
    )]
    InvalidTimestampFormat,
}

/// A version component of an FMRI
///
/// A version consists of:
/// - release: a dot-separated vector of digits (e.g., 5.11)
/// - branch: optional, a dot-separated vector of digits (e.g., 1)
/// - build: optional, a dot-separated vector of digits (e.g., 2020.0.1.0)
/// - timestamp: optional, a hexadecimal string (e.g., 20200421T195136Z)
///
/// The dot-separated vector components (release, branch, build) can be converted to and from
/// semver::Version objects using the provided conversion methods:
/// - release_to_semver
/// - branch_to_semver
/// - build_to_semver
/// - to_semver
///
/// New Version objects can be created from semver::Version objects using:
/// - new_semver
/// - with_branch_semver
/// - with_build_semver
/// - with_timestamp_semver
/// - from_semver
///
/// # Examples
///
/// ```
/// use libips::fmri::Version;
///
/// // Create a Version from strings
/// let version = Version::new("5.11");
///
/// // Convert to semver::Version
/// let semver = version.release_to_semver().unwrap();
/// assert_eq!(semver.major, 5);
/// assert_eq!(semver.minor, 11);
/// assert_eq!(semver.patch, 0);
///
/// // Create a Version from semver::Version
/// let semver_version = semver::Version::new(1, 2, 3);
/// let version = Version::new_semver(semver_version);
/// assert_eq!(version.release, "1.2.3");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Version {
    /// The release component (e.g., 5.11)
    pub release: String,
    /// The branch component (e.g., 1)
    pub branch: Option<String>,
    /// The build component (e.g., 2020.0.1.0)
    pub build: Option<String>,
    /// The timestamp component (e.g., 20200421T195136Z)
    pub timestamp: Option<String>,
}

impl Version {
    /// Create a new Version with the given release
    pub fn new(release: &str) -> Self {
        Version {
            release: release.to_string(),
            branch: None,
            build: None,
            timestamp: None,
        }
    }

    /// Helper method to pad a version string to ensure it has at least MAJOR.MINOR.PATCH components
    ///
    /// This method takes a dot-separated version string and ensures it has at least three components
    /// by padding with zeros if necessary. If the string has more than three components, only the
    /// first three are used.
    fn pad_version_string(version_str: &str) -> String {
        let parts: Vec<&str> = version_str.split('.').collect();
        match parts.len() {
            1 => format!("{}.0.0", parts[0]),
            2 => format!("{}.{}.0", parts[0], parts[1]),
            3 => format!("{}.{}.{}", parts[0], parts[1], parts[2]),
            _ => format!("{}.{}.{}", parts[0], parts[1], parts[2]), // Use only the first three parts
        }
    }

    /// Convert the release component to a semver::Version
    ///
    /// This method attempts to parse the release component as a semver::Version.
    /// If the release component doesn't have enough parts (e.g., "5.11" instead of "5.11.0"),
    /// it will be padded with zeros to make it a valid semver version.
    /// If the release component has more than three parts, only the first three will be used.
    pub fn release_to_semver(&self) -> Result<semver::Version, semver::Error> {
        let version_str = Self::pad_version_string(&self.release);
        version_str.parse()
    }

    /// Convert the branch component to a semver::Version
    ///
    /// This method attempts to parse the branch component as a semver::Version.
    /// If the branch component doesn't have enough parts (e.g., "1" instead of "1.0.0"),
    /// it will be padded with zeros to make it a valid semver version.
    /// If the branch component has more than three parts, only the first three will be used.
    /// Returns None if the branch component is None.
    pub fn branch_to_semver(&self) -> Option<Result<semver::Version, semver::Error>> {
        self.branch.as_ref().map(|branch| {
            let version_str = Self::pad_version_string(branch);
            version_str.parse()
        })
    }

    /// Convert the build component to a semver::Version
    ///
    /// This method attempts to parse the build component as a semver::Version.
    /// If the build component doesn't have enough parts (e.g., "1" instead of "1.0.0"),
    /// it will be padded with zeros to make it a valid semver version.
    /// If the build component has more than three parts, only the first three will be used.
    /// Returns None if the build component is None.
    pub fn build_to_semver(&self) -> Option<Result<semver::Version, semver::Error>> {
        self.build.as_ref().map(|build| {
            let version_str = Self::pad_version_string(build);
            version_str.parse()
        })
    }

    /// Create a new Version with the given semver::Version as release
    ///
    /// This method creates a new Version with the given semver::Version as release.
    /// The semver::Version is converted to a string.
    pub fn new_semver(release: semver::Version) -> Self {
        Version {
            release: release.to_string(),
            branch: None,
            build: None,
            timestamp: None,
        }
    }

    /// Create a Version from semver::Version components
    ///
    /// This method creates a Version from semver::Version components.
    /// The semver::Version components are converted to strings.
    pub fn from_semver(
        release: semver::Version,
        branch: Option<semver::Version>,
        build: Option<semver::Version>,
        timestamp: Option<String>,
    ) -> Self {
        Version {
            release: release.to_string(),
            branch: branch.map(|v| v.to_string()),
            build: build.map(|v| v.to_string()),
            timestamp,
        }
    }

    /// Create a new Version with the given semver::Version as release and branch
    ///
    /// This method creates a new Version with the given semver::Version as release and branch.
    /// The semver::Version objects are converted to strings.
    pub fn with_branch_semver(release: semver::Version, branch: semver::Version) -> Self {
        Version {
            release: release.to_string(),
            branch: Some(branch.to_string()),
            build: None,
            timestamp: None,
        }
    }

    /// Create a new Version with the given semver::Version as release, branch, and build
    ///
    /// This method creates a new Version with the given semver::Version as release, branch, and build.
    /// The semver::Version objects are converted to strings.
    pub fn with_build_semver(
        release: semver::Version,
        branch: Option<semver::Version>,
        build: semver::Version,
    ) -> Self {
        Version {
            release: release.to_string(),
            branch: branch.map(|v| v.to_string()),
            build: Some(build.to_string()),
            timestamp: None,
        }
    }

    /// Create a new Version with the given semver::Version as release, branch, build, and timestamp
    ///
    /// This method creates a new Version with the given semver::Version as release, branch, build, and timestamp.
    /// The semver::Version objects are converted to strings.
    pub fn with_timestamp_semver(
        release: semver::Version,
        branch: Option<semver::Version>,
        build: Option<semver::Version>,
        timestamp: &str,
    ) -> Self {
        Version {
            release: release.to_string(),
            branch: branch.map(|v| v.to_string()),
            build: build.map(|v| v.to_string()),
            timestamp: Some(timestamp.to_string()),
        }
    }

    /// Get all version components as semver::Version objects
    ///
    /// This method returns all version components as semver::Version objects.
    /// If a component is not present or cannot be parsed, it will be None.
    pub fn to_semver(
        &self,
    ) -> (
        Result<semver::Version, semver::Error>,
        Option<Result<semver::Version, semver::Error>>,
        Option<Result<semver::Version, semver::Error>>,
    ) {
        let release = self.release_to_semver();
        let branch = self.branch_to_semver();
        let build = self.build_to_semver();

        (release, branch, build)
    }

    /// Check if this version is compatible with semver
    ///
    /// This method checks if all components of this version can be parsed as semver::Version objects.
    pub fn is_semver_compatible(&self) -> bool {
        let (release, branch, build) = self.to_semver();

        let release_ok = release.is_ok();
        let branch_ok = branch.map_or(true, |r| r.is_ok());
        let build_ok = build.map_or(true, |r| r.is_ok());

        release_ok && branch_ok && build_ok
    }

    /// Create a new Version with the given release and branch
    pub fn with_branch(release: &str, branch: &str) -> Self {
        Version {
            release: release.to_string(),
            branch: Some(branch.to_string()),
            build: None,
            timestamp: None,
        }
    }

    /// Create a new Version with the given release, branch, and build
    pub fn with_build(release: &str, branch: Option<&str>, build: &str) -> Self {
        Version {
            release: release.to_string(),
            branch: branch.map(|b| b.to_string()),
            build: Some(build.to_string()),
            timestamp: None,
        }
    }

    /// Create a new Version with the given release, branch, build, and timestamp
    pub fn with_timestamp(
        release: &str,
        branch: Option<&str>,
        build: Option<&str>,
        timestamp: &str,
    ) -> Self {
        Version {
            release: release.to_string(),
            branch: branch.map(|b| b.to_string()),
            build: build.map(|b| b.to_string()),
            timestamp: Some(timestamp.to_string()),
        }
    }

    /// Parse a version string into a Version
    ///
    /// The version string should be in the format: release\[,branch\]\[-build\]\[:timestamp\]
    pub fn parse(version_str: &str) -> Result<Self, FmriError> {
        let mut version = Version {
            release: String::new(),
            branch: None,
            build: None,
            timestamp: None,
        };

        // Split by colon to separate timestamp
        let parts: Vec<&str> = version_str.split(':').collect();
        if parts.len() > 2 {
            return Err(FmriError::InvalidVersionFormat);
        }

        // If there's a timestamp, parse it
        if parts.len() == 2 {
            let timestamp = parts[1];
            // Reject empty timestamps
            if timestamp.is_empty() {
                return Err(FmriError::InvalidTimestampFormat);
            }
            if !timestamp
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == 'T' || c == 'Z')
            {
                return Err(FmriError::InvalidTimestampFormat);
            }
            version.timestamp = Some(timestamp.to_string());
        }

        // Split the first part by dash to separate build
        let parts: Vec<&str> = parts[0].split('-').collect();
        if parts.len() > 2 {
            return Err(FmriError::InvalidVersionFormat);
        }

        // If there's a build, parse it
        if parts.len() == 2 {
            let build = parts[1];
            if !Self::is_valid_dot_vector(build) {
                return Err(FmriError::InvalidBuildFormat);
            }
            version.build = Some(build.to_string());
        }

        // Split the first part by comma to separate release and branch
        let parts: Vec<&str> = parts[0].split(',').collect();
        if parts.len() > 2 {
            return Err(FmriError::InvalidVersionFormat);
        }

        // Parse the release
        let release = parts[0];
        if !Self::is_valid_dot_vector(release) {
            return Err(FmriError::InvalidReleaseFormat);
        }
        version.release = release.to_string();

        // If there's a branch, parse it
        if parts.len() == 2 {
            let branch = parts[1];
            if !Self::is_valid_dot_vector(branch) {
                return Err(FmriError::InvalidBranchFormat);
            }
            version.branch = Some(branch.to_string());
        }

        Ok(version)
    }

    /// Check if a string is a valid dot-separated vector of digits
    ///
    /// This method uses semver for validation when possible, but also accepts
    /// dot-separated vectors with fewer than 3 components (which are not valid semver).
    fn is_valid_dot_vector(s: &str) -> bool {
        if s.is_empty() {
            return false;
        }

        // First check if it's a valid dot-separated vector of digits
        let parts: Vec<&str> = s.split('.').collect();
        for part in &parts {
            if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
        }

        // If it has at least 3 components, try to parse it as a semver version
        if parts.len() >= 3 {
            // Create a version string with exactly MAJOR.MINOR.PATCH
            let version_str = format!("{}.{}.{}", parts[0], parts[1], parts[2]);

            // Try to parse it as a semver version
            if let Err(_) = semver::Version::parse(&version_str) {
                return false;
            }
        }

        true
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.release)?;

        if let Some(branch) = &self.branch {
            write!(f, ",{}", branch)?;
        }

        if let Some(build) = &self.build {
            write!(f, "-{}", build)?;
        }

        if let Some(timestamp) = &self.timestamp {
            write!(f, ":{}", timestamp)?;
        }

        Ok(())
    }
}

impl FromStr for Version {
    type Err = FmriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// An FMRI (Fault Management Resource Identifier)
///
/// An FMRI is a unique identifier for a package in the IPS system.
/// It follows the format: pkg://publisher/package_name@version
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Fmri {
    /// The scheme (e.g., pkg)
    pub scheme: String,
    /// The publisher (e.g., openindiana.org)
    pub publisher: Option<String>,
    /// The package name (e.g., web/server/nginx)
    pub name: String,
    /// The version
    pub version: Option<Version>,
}

impl Fmri {
    /// Create a new FMRI with the given name
    pub fn new(name: &str) -> Self {
        Fmri {
            scheme: "pkg".to_string(),
            publisher: None,
            name: name.to_string(),
            version: None,
        }
    }

    /// Create a new FMRI with the given name and version
    pub fn with_version(name: &str, version: Version) -> Self {
        Fmri {
            scheme: "pkg".to_string(),
            publisher: None,
            name: name.to_string(),
            version: Some(version),
        }
    }

    /// Create a new FMRI with the given publisher, name, and version
    pub fn with_publisher(publisher: &str, name: &str, version: Option<Version>) -> Self {
        Fmri {
            scheme: "pkg".to_string(),
            publisher: Some(publisher.to_string()),
            name: name.to_string(),
            version,
        }
    }

    /// Get the stem of the FMRI (the package name without version)
    pub fn stem(&self) -> &str {
        &self.name
    }

    /// Get the version of the FMRI as a string
    pub fn version(&self) -> String {
        match &self.version {
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }

    /// Parse an FMRI string into an Fmri
    ///
    /// The FMRI string should be in the format: \[scheme://\]\[publisher/\]name[@version]
    pub fn parse(fmri_str: &str) -> Result<Self, FmriError> {
        let mut fmri = Fmri {
            scheme: "pkg".to_string(),
            publisher: None,
            name: String::new(),
            version: None,
        };

        // Split by @ to separate name and version
        let parts: Vec<&str> = fmri_str.split('@').collect();
        if parts.len() > 2 {
            return Err(FmriError::InvalidFormat);
        }

        // If there's a version, parse it
        if parts.len() == 2 {
            let version = Version::parse(parts[1])?;
            fmri.version = Some(version);
        }

        // Parse the name part
        let name_part = parts[0];

        // Check if there's a scheme with a publisher (pkg://publisher/name)
        if let Some(scheme_end) = name_part.find("://") {
            fmri.scheme = name_part[0..scheme_end].to_string();

            // Extract the rest after the scheme
            let rest = &name_part[scheme_end + 3..];

            // Check if there's a publisher
            if let Some(publisher_end) = rest.find('/') {
                // If there's a non-empty publisher, set it
                if publisher_end > 0 {
                    fmri.publisher = Some(rest[0..publisher_end].to_string());
                }

                // Set the name
                fmri.name = rest[publisher_end + 1..].to_string();
            } else {
                // No publisher, just a name
                fmri.name = rest.to_string();
            }
        }
        // Check if there's a scheme without a publisher (pkg:/name)
        else if let Some(scheme_end) = name_part.find(":/") {
            fmri.scheme = name_part[0..scheme_end].to_string();

            // Extract the rest after the scheme
            let rest = &name_part[scheme_end + 2..];

            // Set the name
            fmri.name = rest.to_string();
        } else {
            // No scheme, just a name
            fmri.name = name_part.to_string();
        }

        // Validate that the name is not empty
        if fmri.name.is_empty() {
            return Err(FmriError::InvalidFormat);
        }

        Ok(fmri)
    }
}

impl fmt::Display for Fmri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // For FMRIs without a publisher, we should use the format pkg:/name
        // For FMRIs with a publisher, we should use the format pkg://publisher/name
        if let Some(publisher) = &self.publisher {
            write!(f, "{}://{}/", self.scheme, publisher)?;
        } else {
            write!(f, "{}:/", self.scheme)?;
        }

        write!(f, "{}", self.name)?;

        if let Some(version) = &self.version {
            write!(f, "@{}", version)?;
        }

        Ok(())
    }
}

impl FromStr for Fmri {
    type Err = FmriError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_conversion() {
        // Test release_to_semver
        let version = Version::new("5.11");
        let semver = version.release_to_semver().unwrap();
        assert_eq!(semver.major, 5);
        assert_eq!(semver.minor, 11);
        assert_eq!(semver.patch, 0);

        // Test with a full semver version
        let version = Version::new("1.2.3");
        let semver = version.release_to_semver().unwrap();
        assert_eq!(semver.major, 1);
        assert_eq!(semver.minor, 2);
        assert_eq!(semver.patch, 3);

        // Test branch_to_semver
        let version = Version::with_branch("5.11", "1");
        let semver = version.branch_to_semver().unwrap().unwrap();
        assert_eq!(semver.major, 1);
        assert_eq!(semver.minor, 0);
        assert_eq!(semver.patch, 0);

        // Test with a full semver version
        let mut version = Version::new("5.11");
        version.branch = Some("1.2.3".to_string());
        let semver = version.branch_to_semver().unwrap().unwrap();
        assert_eq!(semver.major, 1);
        assert_eq!(semver.minor, 2);
        assert_eq!(semver.patch, 3);

        // Test build_to_semver
        let mut version = Version::new("5.11");
        version.build = Some("2020.0.1.0".to_string());
        let semver = version.build_to_semver().unwrap().unwrap();
        assert_eq!(semver.major, 2020);
        assert_eq!(semver.minor, 0);
        assert_eq!(semver.patch, 1);

        // Test from_semver
        let release = semver::Version::new(5, 11, 0);
        let branch = Some(semver::Version::new(1, 0, 0));
        let build = Some(semver::Version::new(2020, 0, 1));
        let timestamp = Some("20200421T195136Z".to_string());

        let version = Version::from_semver(release, branch, build, timestamp);
        assert_eq!(version.release, "5.11.0");
        assert_eq!(version.branch, Some("1.0.0".to_string()));
        assert_eq!(version.build, Some("2020.0.1".to_string()));
        assert_eq!(version.timestamp, Some("20200421T195136Z".to_string()));
    }

    #[test]
    fn test_new_semver_constructors() {
        // Test new_semver
        let semver_version = semver::Version::new(1, 2, 3);
        let version = Version::new_semver(semver_version);
        assert_eq!(version.release, "1.2.3");
        assert_eq!(version.branch, None);
        assert_eq!(version.build, None);
        assert_eq!(version.timestamp, None);

        // Test with_branch_semver
        let release = semver::Version::new(5, 11, 0);
        let branch = semver::Version::new(1, 0, 0);
        let version = Version::with_branch_semver(release, branch);
        assert_eq!(version.release, "5.11.0");
        assert_eq!(version.branch, Some("1.0.0".to_string()));
        assert_eq!(version.build, None);
        assert_eq!(version.timestamp, None);

        // Test with_build_semver
        let release = semver::Version::new(5, 11, 0);
        let branch = Some(semver::Version::new(1, 0, 0));
        let build = semver::Version::new(2020, 0, 1);
        let version = Version::with_build_semver(release, branch, build);
        assert_eq!(version.release, "5.11.0");
        assert_eq!(version.branch, Some("1.0.0".to_string()));
        assert_eq!(version.build, Some("2020.0.1".to_string()));
        assert_eq!(version.timestamp, None);

        // Test with_timestamp_semver
        let release = semver::Version::new(5, 11, 0);
        let branch = Some(semver::Version::new(1, 0, 0));
        let build = Some(semver::Version::new(2020, 0, 1));
        let timestamp = "20200421T195136Z";
        let version = Version::with_timestamp_semver(release, branch, build, timestamp);
        assert_eq!(version.release, "5.11.0");
        assert_eq!(version.branch, Some("1.0.0".to_string()));
        assert_eq!(version.build, Some("2020.0.1".to_string()));
        assert_eq!(version.timestamp, Some("20200421T195136Z".to_string()));
    }

    #[test]
    fn test_to_semver() {
        // Test to_semver with all components
        let mut version = Version::new("5.11");
        version.branch = Some("1.2.3".to_string());
        version.build = Some("2020.0.1".to_string());

        let (release, branch, build) = version.to_semver();

        assert!(release.is_ok());
        let release = release.unwrap();
        assert_eq!(release.major, 5);
        assert_eq!(release.minor, 11);
        assert_eq!(release.patch, 0);

        assert!(branch.is_some());
        let branch = branch.unwrap().unwrap();
        assert_eq!(branch.major, 1);
        assert_eq!(branch.minor, 2);
        assert_eq!(branch.patch, 3);

        assert!(build.is_some());
        let build = build.unwrap().unwrap();
        assert_eq!(build.major, 2020);
        assert_eq!(build.minor, 0);
        assert_eq!(build.patch, 1);

        // Test is_semver_compatible
        assert!(version.is_semver_compatible());

        // Test with invalid semver
        let mut version = Version::new("5.11");
        version.branch = Some("invalid".to_string());
        assert!(!version.is_semver_compatible());
    }

    #[test]
    fn test_semver_validation() {
        // Test valid dot-separated vectors
        assert!(Version::is_valid_dot_vector("5"));
        assert!(Version::is_valid_dot_vector("5.11"));
        assert!(Version::is_valid_dot_vector("5.11.0"));
        assert!(Version::is_valid_dot_vector("2020.0.1.0"));

        // Test invalid dot-separated vectors
        assert!(!Version::is_valid_dot_vector(""));
        assert!(!Version::is_valid_dot_vector(".11"));
        assert!(!Version::is_valid_dot_vector("5."));
        assert!(!Version::is_valid_dot_vector("5..11"));
        assert!(!Version::is_valid_dot_vector("5a.11"));

        // Test semver validation
        assert!(Version::is_valid_dot_vector("1.2.3"));
        assert!(Version::is_valid_dot_vector("0.0.0"));
        assert!(Version::is_valid_dot_vector("999999.999999.999999"));
    }

    #[test]
    fn test_version_parse() {
        // Test parsing a release
        let version = Version::parse("5.11").unwrap();
        assert_eq!(version.release, "5.11");
        assert_eq!(version.branch, None);
        assert_eq!(version.build, None);
        assert_eq!(version.timestamp, None);

        // Test parsing a release and branch
        let version = Version::parse("5.11,1").unwrap();
        assert_eq!(version.release, "5.11");
        assert_eq!(version.branch, Some("1".to_string()));
        assert_eq!(version.build, None);
        assert_eq!(version.timestamp, None);

        // Test parsing a release, branch, and build
        let version = Version::parse("5.11,1-2020.0.1.0").unwrap();
        assert_eq!(version.release, "5.11");
        assert_eq!(version.branch, Some("1".to_string()));
        assert_eq!(version.build, Some("2020.0.1.0".to_string()));
        assert_eq!(version.timestamp, None);

        // Test parsing a release and build (no branch)
        let version = Version::parse("5.11-2020.0.1.0").unwrap();
        assert_eq!(version.release, "5.11");
        assert_eq!(version.branch, None);
        assert_eq!(version.build, Some("2020.0.1.0".to_string()));
        assert_eq!(version.timestamp, None);

        // Test parsing a release, branch, build, and timestamp
        let version = Version::parse("5.11,1-2020.0.1.0:20200421T195136Z").unwrap();
        assert_eq!(version.release, "5.11");
        assert_eq!(version.branch, Some("1".to_string()));
        assert_eq!(version.build, Some("2020.0.1.0".to_string()));
        assert_eq!(version.timestamp, Some("20200421T195136Z".to_string()));

        // Test parsing a release and timestamp (no branch or build)
        let version = Version::parse("5.11:20200421T195136Z").unwrap();
        assert_eq!(version.release, "5.11");
        assert_eq!(version.branch, None);
        assert_eq!(version.build, None);
        assert_eq!(version.timestamp, Some("20200421T195136Z".to_string()));

        // Test parsing a release, branch, and timestamp (no build)
        let version = Version::parse("5.11,1:20200421T195136Z").unwrap();
        assert_eq!(version.release, "5.11");
        assert_eq!(version.branch, Some("1".to_string()));
        assert_eq!(version.build, None);
        assert_eq!(version.timestamp, Some("20200421T195136Z".to_string()));
    }

    #[test]
    fn test_version_display() {
        // Test displaying a release
        let version = Version::new("5.11");
        assert_eq!(version.to_string(), "5.11");

        // Test displaying a release and branch
        let version = Version::with_branch("5.11", "1");
        assert_eq!(version.to_string(), "5.11,1");

        // Test displaying a release, branch, and build
        let version = Version::with_build("5.11", Some("1"), "2020.0.1.0");
        assert_eq!(version.to_string(), "5.11,1-2020.0.1.0");

        // Test displaying a release and build (no branch)
        let version = Version::with_build("5.11", None, "2020.0.1.0");
        assert_eq!(version.to_string(), "5.11-2020.0.1.0");

        // Test displaying a release, branch, build, and timestamp
        let version =
            Version::with_timestamp("5.11", Some("1"), Some("2020.0.1.0"), "20200421T195136Z");
        assert_eq!(version.to_string(), "5.11,1-2020.0.1.0:20200421T195136Z");

        // Test displaying a release and timestamp (no branch or build)
        let version = Version::with_timestamp("5.11", None, None, "20200421T195136Z");
        assert_eq!(version.to_string(), "5.11:20200421T195136Z");

        // Test displaying a release, branch, and timestamp (no build)
        let version = Version::with_timestamp("5.11", Some("1"), None, "20200421T195136Z");
        assert_eq!(version.to_string(), "5.11,1:20200421T195136Z");
    }

    #[test]
    fn test_fmri_parse() {
        // Test parsing a name only
        let fmri = Fmri::parse("sunos/coreutils").unwrap();
        assert_eq!(fmri.scheme, "pkg");
        assert_eq!(fmri.publisher, None);
        assert_eq!(fmri.name, "sunos/coreutils");
        assert_eq!(fmri.version, None);

        // Test parsing a name and version
        let fmri = Fmri::parse("sunos/coreutils@5.11,1:20200421T195136Z").unwrap();
        assert_eq!(fmri.scheme, "pkg");
        assert_eq!(fmri.publisher, None);
        assert_eq!(fmri.name, "sunos/coreutils");
        assert_eq!(
            fmri.version,
            Some(Version {
                release: "5.11".to_string(),
                branch: Some("1".to_string()),
                build: None,
                timestamp: Some("20200421T195136Z".to_string()),
            })
        );

        // Test parsing with scheme
        let fmri = Fmri::parse("pkg://sunos/coreutils").unwrap();
        assert_eq!(fmri.scheme, "pkg");
        assert_eq!(fmri.publisher, Some("sunos".to_string()));
        assert_eq!(fmri.name, "coreutils");
        assert_eq!(fmri.version, None);

        // Test parsing with scheme and empty publisher
        let fmri = Fmri::parse("pkg:///sunos/coreutils").unwrap();
        assert_eq!(fmri.scheme, "pkg");
        assert_eq!(fmri.publisher, None);
        assert_eq!(fmri.name, "sunos/coreutils");
        assert_eq!(fmri.version, None);

        // Test parsing with scheme, publisher, and version
        let fmri = Fmri::parse(
            "pkg://openindiana.org/web/server/nginx@1.18.0,5.11-2020.0.1.0:20200421T195136Z",
        )
        .unwrap();
        assert_eq!(fmri.scheme, "pkg");
        assert_eq!(fmri.publisher, Some("openindiana.org".to_string()));
        assert_eq!(fmri.name, "web/server/nginx");
        assert_eq!(
            fmri.version,
            Some(Version {
                release: "1.18.0".to_string(),
                branch: Some("5.11".to_string()),
                build: Some("2020.0.1.0".to_string()),
                timestamp: Some("20200421T195136Z".to_string()),
            })
        );

        // Test parsing with scheme and version
        let fmri = Fmri::parse("pkg:/system/library@0.5.11-2020.0.1.19563").unwrap();
        assert_eq!(fmri.scheme, "pkg");
        assert_eq!(fmri.publisher, None);
        assert_eq!(fmri.name, "system/library");
        assert_eq!(
            fmri.version,
            Some(Version {
                release: "0.5.11".to_string(),
                branch: None,
                build: Some("2020.0.1.19563".to_string()),
                timestamp: None,
            })
        );
    }

    #[test]
    fn test_fmri_display() {
        // Test displaying a name only
        let fmri = Fmri::new("sunos/coreutils");
        assert_eq!(fmri.to_string(), "pkg:///sunos/coreutils");

        // Test displaying a name and version
        let version = Version::with_timestamp("5.11", Some("1"), None, "20200421T195136Z");
        let fmri = Fmri::with_version("sunos/coreutils", version);
        assert_eq!(
            fmri.to_string(),
            "pkg:///sunos/coreutils@5.11,1:20200421T195136Z"
        );

        // Test displaying with publisher
        let fmri = Fmri::with_publisher("openindiana.org", "web/server/nginx", None);
        assert_eq!(fmri.to_string(), "pkg://openindiana.org/web/server/nginx");

        // Test displaying with publisher and version
        let version = Version::with_timestamp(
            "1.18.0",
            Some("5.11"),
            Some("2020.0.1.0"),
            "20200421T195136Z",
        );
        let fmri = Fmri::with_publisher("openindiana.org", "web/server/nginx", Some(version));
        assert_eq!(
            fmri.to_string(),
            "pkg://openindiana.org/web/server/nginx@1.18.0,5.11-2020.0.1.0:20200421T195136Z"
        );
    }

    #[test]
    fn test_version_errors() {
        // Test invalid release format
        assert_eq!(Version::parse(""), Err(FmriError::InvalidReleaseFormat));
        assert_eq!(Version::parse(".11"), Err(FmriError::InvalidReleaseFormat));
        assert_eq!(Version::parse("5."), Err(FmriError::InvalidReleaseFormat));
        assert_eq!(
            Version::parse("5..11"),
            Err(FmriError::InvalidReleaseFormat)
        );
        assert_eq!(
            Version::parse("5a.11"),
            Err(FmriError::InvalidReleaseFormat)
        );

        // Test invalid branch format
        assert_eq!(Version::parse("5.11,"), Err(FmriError::InvalidBranchFormat));
        assert_eq!(
            Version::parse("5.11,.1"),
            Err(FmriError::InvalidBranchFormat)
        );
        assert_eq!(
            Version::parse("5.11,1."),
            Err(FmriError::InvalidBranchFormat)
        );
        assert_eq!(
            Version::parse("5.11,1..2"),
            Err(FmriError::InvalidBranchFormat)
        );
        assert_eq!(
            Version::parse("5.11,1a.2"),
            Err(FmriError::InvalidBranchFormat)
        );

        // Test invalid build format
        assert_eq!(Version::parse("5.11-"), Err(FmriError::InvalidBuildFormat));
        assert_eq!(
            Version::parse("5.11-.1"),
            Err(FmriError::InvalidBuildFormat)
        );
        assert_eq!(
            Version::parse("5.11-1."),
            Err(FmriError::InvalidBuildFormat)
        );
        assert_eq!(
            Version::parse("5.11-1..2"),
            Err(FmriError::InvalidBuildFormat)
        );
        assert_eq!(
            Version::parse("5.11-1a.2"),
            Err(FmriError::InvalidBuildFormat)
        );

        // Test invalid timestamp format
        assert_eq!(
            Version::parse("5.11:"),
            Err(FmriError::InvalidTimestampFormat)
        );
        assert_eq!(
            Version::parse("5.11:xyz"),
            Err(FmriError::InvalidTimestampFormat)
        );

        // Test invalid version format
        assert_eq!(
            Version::parse("5.11,1,2"),
            Err(FmriError::InvalidVersionFormat)
        );
        assert_eq!(
            Version::parse("5.11-1-2"),
            Err(FmriError::InvalidVersionFormat)
        );
        assert_eq!(
            Version::parse("5.11:1:2"),
            Err(FmriError::InvalidVersionFormat)
        );
    }

    #[test]
    fn test_fmri_errors() {
        // Test invalid format
        assert_eq!(Fmri::parse(""), Err(FmriError::InvalidFormat));
        assert_eq!(Fmri::parse("pkg://"), Err(FmriError::InvalidFormat));
        assert_eq!(Fmri::parse("pkg:///"), Err(FmriError::InvalidFormat));
        assert_eq!(
            Fmri::parse("pkg://publisher/"),
            Err(FmriError::InvalidFormat)
        );
        assert_eq!(Fmri::parse("@5.11"), Err(FmriError::InvalidFormat));
        assert_eq!(
            Fmri::parse("name@version@extra"),
            Err(FmriError::InvalidFormat)
        );

        // Test invalid version
        assert_eq!(Fmri::parse("name@"), Err(FmriError::InvalidReleaseFormat));
        assert_eq!(
            Fmri::parse("name@5.11,"),
            Err(FmriError::InvalidBranchFormat)
        );
        assert_eq!(
            Fmri::parse("name@5.11-"),
            Err(FmriError::InvalidBuildFormat)
        );
        assert_eq!(
            Fmri::parse("name@5.11:"),
            Err(FmriError::InvalidTimestampFormat)
        );
    }
}
