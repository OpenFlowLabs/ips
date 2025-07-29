# Obsoleted Packages in IPS

This document describes the handling of obsoleted packages in the Image Packaging System (IPS).

## Overview

Obsoleted packages are packages that are no longer maintained or have been replaced by other packages. In previous versions of IPS, obsoleted packages were marked with the `pkg.obsolete` attribute in their manifest, but they remained in the main package repository. This approach had several drawbacks:

1. Obsoleted packages cluttered the repository and catalog
2. They were still visible in package listings and searches
3. There was no structured way to store metadata about why a package was obsoleted or what replaced it

The new approach stores obsoleted packages in a dedicated directory structure, separate from the main package repository. This provides several benefits:

1. Keeps the main repository clean and focused on active packages
2. Provides a structured way to store metadata about obsoleted packages
3. Allows for better organization and management of obsoleted packages
4. Preserves the original manifest for reference

## Directory Structure

Obsoleted packages are stored in the following directory structure:

```
<repository>/obsoleted/
    <publisher>/
        <package-stem>/
            <version>.json     # Metadata about the obsoleted package
            <version>.manifest # Original manifest of the obsoleted package
```

For example, an obsoleted package `pkg://openindiana.org/library/perl-5/postgres-dbi-5100@2.19.3,5.11-2014.0.1.1:20250628T100651Z` would be stored as:

```
<repository>/obsoleted/
    openindiana.org/
        library/perl-5/postgres-dbi-5100/
            2.19.3%2C5.11-2014.0.1.1%3A20250628T100651Z.json
            2.19.3%2C5.11-2014.0.1.1%3A20250628T100651Z.manifest
```

## Metadata Format

The metadata for an obsoleted package is stored in a JSON file with the following structure:

```json
{
  "fmri": "pkg://openindiana.org/library/perl-5/postgres-dbi-5100@2.19.3,5.11-2014.0.1.1:20250628T100651Z",
  "status": "obsolete",
  "obsolescence_date": "2025-07-29T12:22:00Z",
  "deprecation_message": "This package is deprecated. Use library/perl-5/postgres-dbi instead.",
  "obsoleted_by": [
    "pkg://openindiana.org/library/perl-5/postgres-dbi@3.0.0"
  ],
  "metadata_version": 1,
  "content_hash": "sha256-abc123def456..."
}
```

The fields in the metadata are:

- `fmri`: The full FMRI (Fault Management Resource Identifier) of the obsoleted package
- `status`: Always "obsolete" for obsoleted packages
- `obsolescence_date`: The date when the package was marked as obsoleted
- `deprecation_message`: Optional message explaining why the package was obsoleted
- `obsoleted_by`: Optional list of FMRIs that replace this package
- `metadata_version`: Version of the metadata schema (currently 1)
- `content_hash`: Hash of the original manifest content for integrity verification

## CLI Commands

The following CLI commands are available for managing obsoleted packages:

### Mark a Package as Obsoleted

```bash
pkg6repo obsolete-package -s <repository> -p <publisher> -f <fmri> [-m <message>] [-r <replacement-fmri>...]
```

This command:
1. Moves the package from the main repository to the obsoleted directory
2. Creates metadata for the obsoleted package
3. Removes the package from the catalog
4. Rebuilds the repository metadata

**Example:**
```bash
# Mark a package as obsoleted with a deprecation message and replacement package
pkg6repo obsolete-package -s /path/to/repo -p openindiana.org -f "pkg://openindiana.org/library/perl-5/postgres-dbi-5100@2.19.3" \
  -m "This package is deprecated. Use library/perl-5/postgres-dbi instead." \
  -r "pkg://openindiana.org/library/perl-5/postgres-dbi@3.0.0"
```

### List Obsoleted Packages

```bash
pkg6repo list-obsoleted -s <repository> -p <publisher> [-F <format>] [-H] [--page <page>] [--page-size <page_size>]
```

This command lists obsoleted packages for a publisher with optional pagination. The output format can be:
- `table` (default): Tabular format with columns for name, version, and publisher
- `json`: JSON format
- `tsv`: Tab-separated values

Pagination parameters:
- `--page`: Page number (1-based, defaults to 1)
- `--page-size`: Number of packages per page (defaults to 100, use 0 for all packages)

The output includes pagination information (current page, total pages, total count) in all formats.

**Example:**
```bash
# List all obsoleted packages for a publisher in JSON format
pkg6repo list-obsoleted -s /path/to/repo -p openindiana.org -F json

# List all obsoleted packages for a publisher in table format without headers
pkg6repo list-obsoleted -s /path/to/repo -p openindiana.org -H

# List obsoleted packages with pagination (page 2, 20 packages per page)
pkg6repo list-obsoleted -s /path/to/repo -p openindiana.org --page 2 --page-size 20

# List all obsoleted packages in a single page
pkg6repo list-obsoleted -s /path/to/repo -p openindiana.org --page-size 0
```

### Search Obsoleted Packages

```bash
pkg6repo search-obsoleted -s <repository> -p <publisher> -q <pattern> [-F <format>] [-H] [-n <limit>]
```

This command searches for obsoleted packages that match a pattern. The pattern can be a simple substring or a regular expression.

**Example:**
```bash
# Search for obsoleted packages containing "perl" in the name or FMRI
pkg6repo search-obsoleted -s /path/to/repo -p openindiana.org -q "perl"

# Search with a regular expression and limit results to 10
pkg6repo search-obsoleted -s /path/to/repo -p openindiana.org -q "^library/.*" -n 10
```

### Show Obsoleted Package Details

```bash
pkg6repo show-obsoleted -s <repository> -p <publisher> -f <fmri> [-F <format>]
```

This command shows detailed information about an obsoleted package, including:
- FMRI
- Status
- Obsolescence date
- Deprecation message (if any)
- Replacement packages (if any)
- Metadata version
- Content hash

**Example:**
```bash
# Show details of an obsoleted package in JSON format
pkg6repo show-obsoleted -s /path/to/repo -p openindiana.org \
  -f "pkg://openindiana.org/library/perl-5/postgres-dbi-5100@2.19.3" -F json
```

### Restore an Obsoleted Package

```bash
pkg6repo restore-obsoleted -s <repository> -p <publisher> -f <fmri> [--no-rebuild]
```

This command restores an obsoleted package to the main repository:
1. Retrieves the original manifest from the obsoleted package
2. Creates a transaction in the main repository
3. Adds the package to the transaction
4. Commits the transaction
5. Removes the obsoleted package from the obsoleted packages directory
6. Rebuilds the catalog (unless `--no-rebuild` is specified)

**Example:**
```bash
# Restore an obsoleted package to the main repository
pkg6repo restore-obsoleted -s /path/to/repo -p openindiana.org \
  -f "pkg://openindiana.org/library/perl-5/postgres-dbi-5100@2.19.3"
```

### Export Obsoleted Packages

```bash
pkg6repo export-obsoleted -s <repository> -p <publisher> -o <output-file> [-q <pattern>]
```

This command exports obsoleted packages to a JSON file that can be imported into another repository.

**Example:**
```bash
# Export all obsoleted packages for a publisher
pkg6repo export-obsoleted -s /path/to/repo -p openindiana.org -o /path/to/export.json

# Export only obsoleted packages matching a pattern
pkg6repo export-obsoleted -s /path/to/repo -p openindiana.org -o /path/to/export.json -q "perl"
```

### Import Obsoleted Packages

```bash
pkg6repo import-obsoleted -s <repository> -i <input-file> [-p <publisher>]
```

This command imports obsoleted packages from a JSON file created by `export-obsoleted`.

**Example:**
```bash
# Import obsoleted packages from a file
pkg6repo import-obsoleted -s /path/to/repo -i /path/to/export.json

# Import obsoleted packages and override the publisher
pkg6repo import-obsoleted -s /path/to/repo -i /path/to/export.json -p new-publisher
```

## Importing Obsoleted Packages

When importing packages from a pkg5 repository, packages with the `pkg.obsolete` attribute are automatically detected and stored in the obsoleted directory instead of the main repository. This ensures that obsoleted packages are properly handled during import.

## API

The following classes and methods are available for programmatically managing obsoleted packages:

### ObsoletedPackageManager

This class manages obsoleted packages in the repository:

```
pub struct ObsoletedPackageManager {
    base_path: PathBuf,
}

impl ObsoletedPackageManager {
    // Create a new ObsoletedPackageManager
    pub fn new<P: AsRef<Path>>(repo_path: P) -> Self;
    
    // Initialize the obsoleted packages directory structure
    pub fn init(&self) -> Result<()>;
    
    // Store an obsoleted package
    pub fn store_obsoleted_package(
        &self,
        publisher: &str,
        fmri: &Fmri,
        manifest_content: &str,
        obsoleted_by: Option<Vec<String>>,
        deprecation_message: Option<String>,
    ) -> Result<PathBuf>;
    
    // Check if a package is obsoleted
    pub fn is_obsoleted(&self, publisher: &str, fmri: &Fmri) -> bool;
    
    // Get metadata for an obsoleted package
    pub fn get_obsoleted_package_metadata(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<Option<ObsoletedPackageMetadata>>;
    
    // List all obsoleted packages for a publisher
    pub fn list_obsoleted_packages(&self, publisher: &str) -> Result<Vec<Fmri>>;
    
    // Search for obsoleted packages by pattern
    pub fn search_obsoleted_packages(
        &self,
        publisher: &str,
        pattern: &str,
    ) -> Result<Vec<Fmri>>;
    
    // Get and remove an obsoleted package
    pub fn get_and_remove_obsoleted_package(
        &self,
        publisher: &str,
        fmri: &Fmri,
    ) -> Result<String>;
    
    // Export obsoleted packages to a file
    pub fn export_obsoleted_packages(
        &self,
        publisher: &str,
        pattern: Option<&str>,
        output_file: &Path,
    ) -> Result<usize>;
    
    // Import obsoleted packages from a file
    pub fn import_obsoleted_packages(
        &self,
        input_file: &Path,
        override_publisher: Option<&str>,
    ) -> Result<usize>;
}
```

### ObsoletedPackageMetadata

This struct represents metadata for an obsoleted package:

```
pub struct ObsoletedPackageMetadata {
    pub fmri: String,
    pub status: String,
    pub obsolescence_date: String,
    pub deprecation_message: Option<String>,
    pub obsoleted_by: Option<Vec<String>>,
    pub metadata_version: u32,
    pub content_hash: String,
}
```

## Integration with Repository Operations

The obsoleted package system is integrated with the following repository operations:

1. **Package Listing**: Obsoleted packages are excluded from regular package listings
2. **Catalog Building**: Obsoleted packages are excluded from the catalog
3. **Search**: Obsoleted packages are excluded from search results

This ensures that obsoleted packages don't clutter the repository and are properly managed.

## Best Practices for Managing Obsoleted Packages

Here are some best practices for managing obsoleted packages:

### When to Mark a Package as Obsoleted

- **Package is no longer maintained**: When a package is no longer being maintained or updated
- **Package has been replaced**: When a package has been replaced by a newer version or a different package
- **Package is deprecated**: When a package is deprecated and should not be used in new installations
- **Package has security vulnerabilities**: When a package has security vulnerabilities and should not be used

### Providing Useful Metadata

- **Always include a deprecation message**: Explain why the package is obsoleted and what users should do instead
- **Specify replacement packages**: If the package has been replaced, specify the replacement package(s)
- **Be specific about versions**: If only certain versions are obsoleted, be clear about which ones

### Managing Large Numbers of Obsoleted Packages

- **Use batch operations**: Use the export/import commands to manage large numbers of obsoleted packages
- **Use search to find related packages**: Use the search command to find related packages that might also need to be obsoleted
- **Organize by publisher**: Keep obsoleted packages organized by publisher

### Repository Maintenance

- **Regularly clean up obsoleted packages**: Remove obsoleted packages that are no longer needed
- **Export obsoleted packages before repository cleanup**: Export obsoleted packages before cleaning up a repository
- **Rebuild catalogs after bulk operations**: Rebuild catalogs after bulk operations to ensure consistency

## Troubleshooting

Here are solutions to common issues when working with obsoleted packages:

### Package Not Found in Obsoleted Directory

**Issue**: A package that was marked as obsoleted cannot be found in the obsoleted directory.

**Solution**:
1. Check that the FMRI is correct, including the version and timestamp
2. Verify that the publisher name is correct
3. Use the `search-obsoleted` command with a broader pattern to find similar packages
4. Check the repository logs for any errors during the obsolete operation

### Errors During Import/Export

**Issue**: Errors occur when importing or exporting obsoleted packages.

**Solution**:
1. Ensure the input/output file paths are correct and writable
2. Check that the repository exists and is accessible
3. Verify that the publisher exists in the repository
4. For import errors, check that the JSON file is valid and has the correct format

### Catalog Issues After Restoring Packages

**Issue**: Catalog issues after restoring obsoleted packages to the main repository.

**Solution**:
1. Rebuild the catalog manually using `pkg6repo rebuild`
2. Check for any errors during the rebuild process
3. Verify that the package was properly restored to the main repository
4. Check for any conflicts with existing packages

### Performance Issues with Large Repositories

**Issue**: Performance issues when working with large repositories with many obsoleted packages.

**Solution**:
1. Use the search command with specific patterns to limit the number of packages processed
2. Use pagination when listing or searching for obsoleted packages
3. Export obsoleted packages to separate files by category or pattern
4. Consider using a more powerful machine for repository operations

## Workflow Diagram

Here's a simplified workflow for managing obsoleted packages:

```
                                 +-------------------+
                                 | Active Repository |
                                 +-------------------+
                                          |
                                          | obsolete-package
                                          v
                                 +-------------------+
                                 | Obsoleted Storage |
                                 +-------------------+
                                          |
                                          | (manage)
                                          v
                      +------------------------------------------+
                      |                                          |
          +-----------+-----------+                  +-----------+-----------+
          | list-obsoleted        |                  | search-obsoleted      |
          | show-obsoleted        |                  | export-obsoleted      |
          +-----------------------+                  +-----------------------+
                      |                                          |
                      v                                          v
          +-----------------------+                  +-----------------------+
          | restore-obsoleted     |                  | import-obsoleted      |
          +-----------------------+                  +-----------------------+
                      |                                          |
                      v                                          v
          +-----------------------+                  +-----------------------+
          | Back to Active Repo   |                  | Different Repository  |
          +-----------------------+                  +-----------------------+
```

This diagram illustrates the flow of packages between the active repository and the obsoleted storage, as well as the various commands used to manage obsoleted packages.