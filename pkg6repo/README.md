# pkg6repo

pkg6repo is a Rust implementation of the Image Packaging System (IPS) repository management utility. It is designed to replace the pkgrepo command from the original IPS implementation.

## Installation

To build and install pkg6repo, you need to have Rust and Cargo installed. Then, you can build the project using:

```bash
cargo build --release
```

The binary will be available at `target/release/pkg6repo`.

## Usage

pkg6repo provides several subcommands for managing package repositories:

### Create a Repository

Create a new package repository:

```bash
pkg6repo create /path/to/repository
```

You can specify the repository version (default is 4):

```bash
pkg6repo create --version 4 /path/to/repository
```

### Add Publishers

Add publishers to a repository:

```bash
pkg6repo add-publisher -s /path/to/repository example.com
```

### Remove Publishers

Remove publishers from a repository:

```bash
pkg6repo remove-publisher -s /path/to/repository example.com
```

You can perform a dry run to see what would be removed without actually removing anything:

```bash
pkg6repo remove-publisher -n -s /path/to/repository example.com
```

### Get Repository Properties

Get repository properties:

```bash
pkg6repo get -s /path/to/repository
```

You can specify specific properties to get:

```bash
pkg6repo get -s /path/to/repository publisher/prefix
```

### Set Repository Properties

Set repository properties:

```bash
pkg6repo set -s /path/to/repository publisher/prefix=example.com
```

You can set publisher-specific properties:

```bash
pkg6repo set -s /path/to/repository -p example.com repository/origins=http://example.com/repository
```

### Display Repository Information

Display information about a repository:

```bash
pkg6repo info -s /path/to/repository
```

### List Packages

List packages in a repository:

```bash
pkg6repo list -s /path/to/repository
```

You can filter by publisher:

```bash
pkg6repo list -s /path/to/repository -p example.com
```

You can also filter by package pattern:

```bash
pkg6repo list -s /path/to/repository example/package
```

### Show Package Contents

Show contents of packages in a repository:

```bash
pkg6repo contents -s /path/to/repository
```

You can filter by package pattern:

```bash
pkg6repo contents -s /path/to/repository example/package
```

You can also filter by action type:

```bash
pkg6repo contents -s /path/to/repository -t file
```

### Rebuild Repository Metadata

Rebuild repository metadata:

```bash
pkg6repo rebuild -s /path/to/repository
```

You can skip catalog or index rebuilding:

```bash
pkg6repo rebuild -s /path/to/repository --no-catalog
pkg6repo rebuild -s /path/to/repository --no-index
```

### Refresh Repository Metadata

Refresh repository metadata:

```bash
pkg6repo refresh -s /path/to/repository
```

You can skip catalog or index refreshing:

```bash
pkg6repo refresh -s /path/to/repository --no-catalog
pkg6repo refresh -s /path/to/repository --no-index
```

## License

This project is licensed under the same license as the original IPS implementation.