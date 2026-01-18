# Image Packaging System for illumos

## libips
The libips crate contains all modules and functions one needs to implement an Image Packaging System based utility.

Be that a server, client or other utilities. 

Includes Python bindings with PyO3.

This project is intended to gradually replace the 
[current python based implementation of IPS](https://github.com/openindiana/pkg5). 
Most things are documented in the [docs](https://github.com/OpenIndiana/pkg5/tree/oi/doc) directory 
but some things have been added over the years which has not been properly documented. Help is welcome 
but be advised, this is mainly intended for use within the illumos community and it's distributions.
Big changes which are not in the current IPS will need to be carefully coordinated to not break the current
IPS.

## Development and Release

### Releasing

This project uses `cargo-release` for versioning and tagging.

1. Ensure you have `cargo-release` installed: `cargo install cargo-release`
2. Prepare the release (dry-run): `cargo release [level] --dry-run` (e.g., `cargo release patch --dry-run`)
3. Execute the release: `cargo release [level] --execute`
4. Push the changes and tags: `git push --follow-tags`

Pushing a tag starting with `v` (e.g., `v0.5.1`) will trigger the GitHub Actions release pipeline, which builds artifacts for Illumos (OpenIndiana) and Linux, and creates a GitHub Release.
