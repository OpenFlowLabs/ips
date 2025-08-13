### Continuation: What’s still missing to actually install into a User or Partial Image

Below is a concrete, actionable continuation that builds on the current libips Image/catalog/installed DB work and outlines what remains to support real installs into User or Partial images. It follows the project’s error-handling guidelines (thiserror + miette; no “fancy” in lib crates), and suggests minimal APIs and milestones.

---

### What’s implemented today (recap)
- Image structure and metadata
    - Full vs Partial images and metadata paths (var/pkg vs .pkg)
    - Publisher add/remove/default, persisted in pkg6.image.json
- Catalogs
    - Download per-publisher catalogs via RestBackend
    - Build a merged image catalog (ImageCatalog) and query packages
    - Retrieve manifests reconstructed from catalogs
- Installed DB
    - Installed packages redb with add/remove/query/get-manifest
    - Keys are full FMRI strings with publisher
- Errors
    - ImageError, InstalledError, Catalog errors use thiserror + miette (no fancy)

This is a solid foundation for discovery and state, but doesn’t yet apply manifests to the filesystem or fetch payloads, which are required for actual installs.

---

### Missing components for real installation
1) Dependency resolution and planning
- Need a solver that, given requested specs, picks package versions, resolves require dependencies, excludes obsolete/renamed where appropriate, and produces an InstallPlan.

2) Payload fetching
- RestBackend currently fetches catalogs only; it needs a method to fetch content payloads (files) by digest/hash to a local cache, with verification.

3) Action executor (filesystem apply)
- Implement an installer that interprets Manifest actions (Dir, File, Link, etc.) relative to the image root, writes files atomically, sets modes/owners/groups, and updates the Installed DB upon success.

4) Transaction/locking and rollback
- Image-level lock to serialize operations; minimal rollback with temp files or a small journal.

5) Uninstall/update planning and execution
- Compute diffs vs installed manifests; remove safely; preserve config files where appropriate; perform updates atomically.

6) Partial/User image policy
- Define which actions are permitted in partial/user images (likely restrict Users/Groups/Drivers, etc.) and enforce with clear diagnostics.

7) Security and verification (future)
- TLS settings for repos, signature verification for catalogs and payloads.

8) CLI wiring
- pkg6 install/uninstall/update subcommands calling into libips high-level APIs.

9) Tests
- Unit: executor; Integration: mock repo + payloads; E2E: cargo xtask setup-test-env.

---

### Proposed modules and APIs

#### 1. Solver
- Location: libips/src/solver/mod.rs
- Types
    - ResolvedPkg { fmri: Fmri, manifest: actions::Manifest }
    - Constraint { stem: String, version_req: Option<String>, publisher: Option<String> }
    - InstallPlan { add: Vec<ResolvedPkg>, remove: Vec<ResolvedPkg>, update: Vec<(ResolvedPkg, ResolvedPkg)>, reasons: Vec<String> }
- Error
    - SolverError (thiserror + miette, no fancy), code prefix ips::solver_error
- Functions
    - fn resolve_install(image: &Image, constraints: &[Constraint]) -> Result<InstallPlan, SolverError>
- MVP behavior
    - Choose highest non-obsolete version matching constraints; fetch manifests via Image::get_manifest_from_catalog; perform require dependency closure; error on missing deps.

#### 2. Payload fetching
- Extend repository API
    - trait ReadableRepository add:
        - fn fetch_payload(&mut self, publisher: &str, digest: &str, dest: &Path) -> Result<(), RepositoryError>
    - Or introduce a small RepositorySource used by installer to abstract fetching/caching.
- RestBackend implementation
    - Derive URL for payloads by digest; download to temp; verify with crate::digest; move into cache.
- Image helpers
    - fn content_cache_dir(&self) -> PathBuf (e.g., metadata_dir()/content)
    - fn ensure_payload(&self, digest: &Digest) -> Result<PathBuf, ImageError>

Note: Ensure file actions in manifests include digest/hash attributes. If current catalog->manifest synthesis drops them, extend it so actions::File carries digest, size, mode, owner, group, path.

#### 3. Action executor
- Location: libips/src/apply/mod.rs
- Types
    - ApplyOptions { dry_run: bool, preserve_configs: bool, no_backup: bool }
    - InstallerError (thiserror + miette), code ips::installer_error
- Functions
    - fn apply_install_plan(image: &Image, plan: &InstallPlan, repo_src: &mut impl RepositorySource, opts: &ApplyOptions) -> Result<(), InstallerError>
- Handling (MVP)
    - Dir: create with mode/owner/group
    - File: fetch payload; write to temp; fsync; set metadata; rename atomically
    - Link: create symlink/hardlink
    - Attr/License: metadata only (store or ignore initially)
- Policy for Partial images
    - Forbid user/group creation and other privileged actions; return ValidationError (ips::validation_error::forbidden_action)

#### 4. High-level Image orchestration
- New APIs on Image
    - fn plan_install(&self, specs: &[String]) -> Result<InstallPlan, ImageError>
    - fn apply_plan(&self, plan: &InstallPlan, opts: &ApplyOptions) -> Result<(), ImageError>
    - fn install(&self, specs: &[String], opts: &ApplyOptions) -> Result<(), ImageError>
- Behavior
    - Acquire per-image lock (metadata_dir()/image.lock)
    - Resolve plan; ensure payloads; apply; on success, update Installed DB via existing methods

#### 5. Uninstall and update
- Plan functions similar to install; compute diffs using old vs new manifests (actions::Diff exists to help)
- Track per-package installed file list for precise removal; can derive from manifest for MVP.

---

### Minimal milestone sequence (practical path)
- Milestone A: “Hello-world” install into a temp Partial image
    1) Ensure file actions include digest in manifests
    2) Add RestBackend::fetch_payload + Image cache
    3) Implement executor for Dir/File/Link
    4) Image::install that resolves a single package without deps and applies
    5) Update Installed DB only after filesystem success

- Milestone B: Basic dependency closure and uninstall
    1) MVP solver for require deps
    2) Per-package file tracking; uninstall using that
    3) Image lock; dry-run flag
    4) Tests for partial image policy and path isolation

- Milestone C: Updates and diagnostics
    1) Diff-based updates; safe replacement
    2) Improved miette diagnostics with codes and help
    3) CLI commands in pkg6 with fancy feature

---

### Error handling alignment
- New error enums:
    - SolverError: ips::solver_error::{missing_dependency, conflict, …}
    - InstallerError: ips::installer_error::{io, forbidden_action, payload_missing, …}
    - ValidationError (if separate): ips::validation_error::{forbidden_action, invalid_spec, …}
- Library code uses specific error types; app code (pkg6) may use miette::Result with fancy.

Example variant:
- #[diagnostic(code(ips::installer_error::forbidden_action), help("Remove this package or use a full image"))]

---

### Open items to confirm
- Exact allowed action set for Partial/User images?
- Payload cache location and retention policy; proposed metadata_dir()/content with hash sharding
- REST payload URL structure (by digest) for your repos; adjust RestBackend accordingly

If you can confirm the above policy and repository layout for payloads, I can draft precise function signatures and a skeleton module structure next.
