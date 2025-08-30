# IPS Library Integration Plan for pkgdev (struct-first)

This document updates the earlier integration plan to align with moving away from text representations (filelist.fmt, .p5m strings, pkgfmt) and toward constructing typed manifests and packages directly via the Rust IPS library (libips). Publishing and repository operations should be performed using the PublisherClient provided by libips.

Repository: https://github.com/OpenFlowLabs/ips.git

Key goals:
- No intermediate text filelists/manifests during the in-memory pipeline.
- Build typed Manifest/Action structures from prototype directories and recipe data.
- Apply transform rules programmatically (instead of pkgmogrify text transforms).
- Generate and resolve dependencies via library analyzers/resolvers.
- Use a typed Repository and PublisherClient to publish transactions atomically.

## Targeted Replacements and Proposed (Typed) APIs

Note: Names are indicative. The libips repository should expose these or equivalent types. Error handling should follow our project guidelines (thiserror + miette::Diagnostic) with codes like `ips::category_error`.

### Core Types (in libips)

- enum Action
  - File { path: PathBuf, mode: Option<u32>, owner: Option<String>, group: Option<String>, hash: Option<String>, ... }
  - Dir { path: PathBuf, ... }
  - Link { path: PathBuf, target: PathBuf, ... }
  - Hardlink { path: PathBuf, target: PathBuf, ... }
  - License { path: PathBuf, license: String }
  - Set { name: String, value: String }
  - Depend { type_: String, fmri: String }
  - … (other IPS actions as needed)

- struct Manifest { actions: Vec<Action> }
  - Methods: validate(), to_string() (for debug/export only), from_string() (optional for interop)

- struct ManifestBuilder
  - from_prototype_dir(proto: &Path) -> Result<Manifest, IpsError>
  - with_base_metadata(meta: BaseMeta) -> &mut Self
  - apply_rules(rules: &[TransformRule]) -> Result<&mut Self, IpsError>
  - build() -> Manifest

- struct TransformRule
  - Selectors and operations analogous to pkgmogrify, but typed and programmatic.

- struct DependencyGenerator
  - generate(proto: &Path, manifest: &Manifest) -> Result<Manifest, IpsError>
    - Adds Depend actions into the provided manifest (returns a new manifest or mutates in place).

- struct Resolver
  - resolve(manifests: &mut [Manifest]) -> Result<(), IpsError>
    - Converts provisional dependency notations into resolved FMRIs across the set.

- struct Repository
  - open(path: &Path) -> Result<Repository, IpsError>
  - create(path: &Path) -> Result<Repository, IpsError>
  - has_publisher(name: &str) -> Result<bool, IpsError>
  - add_publisher(name: &str) -> Result<(), IpsError>

- struct PublisherClient
  - new(repo: Repository, publisher: String) -> Result<PublisherClient, IpsError>
  - begin() -> Result<Txn, IpsError>

- struct Txn
  - add_payload_dir(dir: &Path) -> Result<(), IpsError>
  - add_manifest(manifest: &Manifest) -> Result<(), IpsError>
  - commit() -> Result<(), IpsError>

### 1) Generate Manifest actions from prototype (replaces `pkgsend generate` + `pkgfmt`)

- API: ManifestBuilder::from_prototype_dir(proto)
- In pkgdev: call into libips to produce a Manifest (no filelist.fmt). Save only when exporting/debugging.

### 2) Apply transforms (replaces `pkgmogrify` + `pkgfmt`)

- API: ManifestBuilder::apply_rules(rules)
- In pkgdev: translate recipe/gate transforms into TransformRule structs and apply to the manifest.

### 3) Generate dependencies (replaces `pkgdepend generate` + `pkgfmt`)

- API: DependencyGenerator::generate(proto, &manifest) -> Manifest
- In pkgdev: obtain a Manifest with Depend actions injected.

### 4) Resolve dependencies against repository (replaces `pkgdepend resolve`)

- API: Resolver::resolve_with_repo(repo, publisher, &mut manifests)
- In pkgdev: call resolver with a repository handle so dependencies resolve to already-published packages.

### 5) Lint manifests (replaces `pkglint`)

- API: lint::lint_manifest(&manifest)
- In pkgdev: pass Manifest (typed), receive diagnostics.

### 6) Repository and publisher management (replaces `pkgrepo create/add-publisher`)

- APIs on Repository: create/open/has_publisher/add_publisher
- In pkgdev: ensure repo exists and publisher is present via libips types.

### 7) Publish (replaces `pkgsend publish`)

- APIs: Repository::open/create; PublisherClient::new; Txn::begin -> add_payload_dir -> add_manifest -> commit
- In pkgdev: feed prototype/unpack/pkg dirs via add_payload_dir; add each Manifest; commit.

## Integration points in pkgdev (cfg(feature = "libips"))

The following functions in crates/pkgdev/src/build/ips.rs will switch to libips when the feature is enabled. For now:
- pkgdev constructs a pre-filled typed Manifest struct for base metadata (fmri, summary, classification, upstream/source URLs, license) and writes a debug export as <name>-typed.manifest.json in the manifest directory. This is a temporary internal representation until libips types are available.
- Other libips paths return descriptive TODO errors pointing here.

When implementing with real libips types:
- run_generate_filelist: Build a Manifest via ManifestBuilder::from_prototype_dir (no filelist.fmt). Optionally serialize for debug output only.
- generate_manifest_files: Build base metadata into Manifest; compute TransformRule set from gate and recipe; apply via ManifestBuilder::apply_rules.
- run_generate_pkgdepend: Use DependencyGenerator::generate to add Depend actions to the Manifest; avoid text .dep intermediates.
- run_resolve_dependencies: Use Resolver::resolve_with_repo(repo, publisher, &mut manifests) so dependencies are resolved against a repository of published packages.
- run_lint: Call lint::lint_manifest(&Manifest) and convert diagnostics to miette.
- ensure_repo_with_publisher_exists: Use Repository::{create,has_publisher,add_publisher}.
- publish: Use PublisherClient with a Txn to add payload dirs and the Manifest, then commit.

## Error Handling Guidance

- Define specific error enums with thiserror + miette::Diagnostic.
- Codes: `ips::validation_error`, `ips::repo_error`, `ips::publish_error`, with specific variants (e.g., `ips::repo_error::publisher_missing`).
- Provide helpful diagnostics, including labels for problematic manifest actions when applicable.

## Next Steps in OpenFlowLabs/ips

1. Establish the crate module layout and expose the typed APIs outlined above.
2. Implement modules incrementally:
   - manifest::{Action, Manifest, ManifestBuilder, TransformRule}
   - dep::{DependencyGenerator, Resolver}
   - lint::lint_manifest
   - repo::{Repository, PublisherClient, Txn}
3. Mirror CLI behaviors with tests; provide conversion utilities to import/export .p5m where needed for interop only.
4. Once available, update pkgdev’s libips feature to depend on the crate and replace TODO stubs with real calls.

## Enabling Integration in pkgdev

- The Cargo feature `libips` is defined but does not yet pull the dependency. When libips stabilizes, add it as an optional dependency in crates/pkgdev/Cargo.toml, and wire cfg(feature = "libips") paths to call the typed APIs.
- Avoid introducing new text-based intermediates in the libips path; persist manifests only for debugging/exports.


## Gate KDL Transform Configuration (AST)

The gate crate now supports defining transform rules in KDL and exposes them as a typed AST rather than plain text lines. This enables constructing typed libips TransformRule objects in the future without round-tripping through pkgmogrify text.

KDL schema excerpt within a transform node:

- transform
  - [legacy] positional arguments: textual mogrify lines (kept for compatibility)
  - [legacy] include: optional include file path (string)
  - rule (0..N)
    - select (0..N): properties
      - action: optional string (e.g., "file", "link", "hardlink", "dir")
      - attr: optional string (e.g., "path", "mode", "owner")
      - pattern: optional string (glob/regex or exact string)
    - op (0..N): first argument is operation name; properties:
      - key: optional string (e.g., attribute name for set/delete/default)
      - value: optional string (e.g., value for set/default)

Example:

transform {
  rule {
    select action="file" attr="path" pattern=".*"
    op "set" key="keep" value="true"
  }
}

In Rust (gate crate):
- Transform has a field `rules: Vec<TransformRuleAst>`; use `ast_rules()` to access.
- Legacy fields `actions: Vec<String>` and `include: Option<String>` remain available and are used by existing pkgdev text-based paths.

When libips exposes typed TransformRule, pkgdev can map TransformRuleAst to libips structures directly.


## Additional integration points to audit in this repo

This section lists other areas in the forge codebase that should be considered when replacing CLI tools with libips and moving to typed, in-memory manifests.

1) pkgdev build orchestrator
- File: crates/pkgdev/src/build/mod.rs
- Function: run_ips_actions(...)
- Role: Orchestrates the full IPS pipeline: generate filelist -> build manifests -> deps generate -> deps resolve -> lint -> repo/publisher ensure -> publish.
- Libips path notes:
  - Transition from file-based intermediates to in-memory Manifest values passed between steps.
  - Consider adjusting function signatures to return and accept typed Manifests and dependency results rather than file paths.
  - Preserve step ordering; add structured logging around each lib call.

2) pkgdev development dependency helper (pkg(1) CLI usage)
- File: crates/pkgdev/src/build/dependencies.rs
- Current behavior: Shells out to pkg list and pfexec pkg install to ensure dev dependencies are present on the build host.
- Potential libips integration:
  - If libips exposes image management APIs, consider:
    - struct Image { open(root: &Path) -> Result<Image, IpsError>; list_installed() -> Result<Vec<Fmri>, IpsError>; }
    - impl Image { begin() -> Result<ImageTxn, IpsError> } and ImageTxn::install(pkgs: &[Fmri]) -> Result<(), IpsError>
  - Otherwise, keep CLI for now and document this as out-of-scope for publisher/repo features.
- Documentation gap for libips: clarify whether host image management is in scope.

3) Typed base metadata/template construction
- Location: crates/pkgdev/src/build/ips.rs::generate_manifest_files (libips feature path writes <name>-typed.manifest.json for now).
- Libips API expectations:
  - struct BaseMeta { fmri: Fmri, summary: String, classification: String, upstream_url: Url, source_url: Url, license: LicenseSpec }
  - struct Fmri { pkg: String, version: Semver-ish; build, branch, revision components } with a Display/Parse.
  - ManifestBuilder::with_base_metadata(BaseMeta).

4) Transform include semantics from CLI flags
- Location: pkgdev BuildArgs includes an optional include dir (-I) forwarded to pkgmogrify in the CLI path.
- Libips mapping:
  - Gate KDL AST (see Gate TransformRuleAst) should be primary.
  - For legacy include files, libips could expose a loader/parser for transform files, or pkgdev should translate those into typed TransformRule values.

5) Dependency resolution across multi-package builds
- Location: Orchestrator collects multiple ManifestCollection items and resolves dependencies together.
- Libips mapping:
  - Resolver::resolve(&mut [Manifest]) should support a set of manifests at once so inter-package deps are satisfied without temporary .dep/.dep.res files.
  - Provide diagnostics that identify unresolved FMRIs and which manifest/action caused them.

6) Lint configuration and reference catalogs
- Current CLI path uses pkglint possibly with cache/reference repos (see sample_data/make-rules for patterns).
- Libips mapping:
  - LintConfig { reference_repos: Vec<PathBuf>, rulesets: Vec<String> } and lint::lint_manifest(manifest, &LintConfig).
  - Ensure ability to suppress/waive or categorize warnings vs errors for CI.

7) Repository and publisher management lifecycles
- Current path: ensure_repo_with_publisher_exists uses pkgrepo create/add-publisher and path checks.
- Libips mapping:
  - Repository::create/open, Repository::has_publisher/add_publisher, and helpers for repository initialization (e.g., metadata files, permissions).
  - Consider explicit error variants: ips::repo_error::{not_found, invalid_format, publisher_missing}.

8) Publishing multiple manifests and payload dirs
- Location: pkgdev publish step publishes each manifest with multiple -d payload directories.
- Libips mapping:
  - PublisherClient::new(repo, publisher).begin() -> Txn.
  - Txn::add_payload_dir for prototype, unpack, and pkg directories.
  - Txn::add_manifest for each typed Manifest; support multiple manifests per txn or separate txns.
  - Txn::commit with atomic behavior and clear diagnostics.

9) Sample make-rules parity (reference only)
- Files: sample_data/make-rules/*.mk and sample_data/components/**.p5m
- Purpose: Serve as behavioral reference for transform chains, dependency generation, and publishing patterns.
- Action: Do not wire into pkgdev; use to define tests in libips and to validate parity.

10) Workspace layout and persistence decisions
- Files: crates/workspace (not IPS-specific but used heavily by pkgdev)
- Notes:
  - Under libips, avoid persisting intermediate files except for debug/export.
  - Ensure functions that currently assume files exist (e.g., reading .dep.res) are refactored to accept in-memory Manifests.

11) Future optional: Conversion utilities for interop
- Provide import/export for .p5m and .dep/.dep.res to ease migration and to compare outputs in tests.
- This is already noted, but emphasize the need for round-trip tests in libips.

12) Logging and diagnostics
- Adopt tracing across pkgdev paths interacting with libips.
- Ensure libips returns miette Diagnostics with codes per guidelines to integrate well in pkgdev error reporting.
