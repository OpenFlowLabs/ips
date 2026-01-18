//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

//! Dependency resolution and planning over the ImageCatalog using resolvo.
//! This module implements a resolvo::DependencyProvider with IPS-specific
//! selection rules:
//! - Package identity uses IPS stems and publishers.
//! - Ignore obsolete packages.
//! - Branch is locked to the dependant when resolving dependencies.
//! - Version requirements match on the release component; ordering prefers
//!   newest release, then publisher preference, then timestamp.
//!
//! resolve_install builds a resolvo Problem from user constraints, runs the
//! solver, and assembles an InstallPlan from the chosen solvables.

use miette::Diagnostic;
// Begin resolvo wiring imports (names discovered by compiler)
// We start broad and refine with compiler guidance.
use lz4::Decoder as Lz4Decoder;
use redb::{ReadableDatabase, ReadableTable};
use resolvo::{
    self, Candidates, Condition, ConditionId, ConditionalRequirement,
    Dependencies as RDependencies, DependencyProvider, HintDependenciesAvailable, Interner,
    KnownDependencies, Mapping, NameId, Problem as RProblem, SolvableId, Solver as RSolver,
    SolverCache, StringId, UnsolvableOrCancelled, VersionSetId, VersionSetUnionId,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::io::{Cursor, Read};
use thiserror::Error;

use crate::actions::Manifest;
use crate::image::catalog::{CATALOG_TABLE, INCORPORATE_TABLE};

// Public advice API lives in a sibling module
pub mod advice;

// Local helpers to decode manifest bytes stored in catalog DB (JSON or LZ4-compressed JSON)
fn is_likely_json_local(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\n' | b'\r' | b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return false;
    }
    matches!(bytes[i], b'{' | b'[')
}

fn decode_manifest_bytes_local(bytes: &[u8]) -> Result<Manifest, serde_json::Error> {
    if is_likely_json_local(bytes) {
        return serde_json::from_slice::<Manifest>(bytes);
    }
    // Try LZ4; on failure, fall back to JSON attempt
    if let Ok(mut dec) = Lz4Decoder::new(Cursor::new(bytes)) {
        let mut out = Vec::new();
        if dec.read_to_end(&mut out).is_ok() {
            if let Ok(m) = serde_json::from_slice::<Manifest>(&out) {
                return Ok(m);
            }
        }
    }
    // Fallback to JSON parse of original bytes
    serde_json::from_slice::<Manifest>(bytes)
}

#[derive(Clone, Debug)]
struct PkgCand {
    #[allow(dead_code)]
    id: SolvableId,
    name_id: NameId,
    fmri: Fmri,
}

#[derive(Clone, Debug)]
enum VersionSetKind {
    Any,
    ReleaseEq(String),
    BranchEq(String),
    ReleaseAndBranch { release: String, branch: String },
}

struct IpsProvider<'a> {
    image: &'a Image,
    // Persistent database handles and read transactions for catalog/obsoleted
    _catalog_db: redb::Database,
    catalog_tx: redb::ReadTransaction,
    _obsoleted_db: redb::Database,
    _obsoleted_tx: redb::ReadTransaction,
    // interner storages
    names: Mapping<NameId, String>,
    name_by_str: BTreeMap<String, NameId>,
    strings: Mapping<StringId, String>,
    solvables: Mapping<SolvableId, PkgCand>,
    cands_by_name: HashMap<NameId, Vec<SolvableId>>,
    // Version set storage needs interior mutability to allocate during async trait calls
    version_sets: RefCell<Mapping<VersionSetId, VersionSetKind>>,
    vs_name: RefCell<Mapping<VersionSetId, NameId>>,
    unions: RefCell<Mapping<VersionSetUnionId, Vec<VersionSetId>>>,
    // per-name publisher preference order; set by dependency processing or top-level specs
    publisher_prefs: RefCell<HashMap<NameId, Vec<String>>>,
}
use crate::fmri::Fmri;
use crate::image::Image;

impl<'a> IpsProvider<'a> {
    fn new(image: &'a Image) -> Result<Self, SolverError> {
        // Open databases and keep read transactions alive for the provider lifetime
        let catalog_db = redb::Database::open(image.catalog_db_path())
            .map_err(|e| SolverError::new(format!("open catalog db: {}", e)))?;
        let catalog_tx = catalog_db
            .begin_read()
            .map_err(|e| SolverError::new(format!("begin read catalog db: {}", e)))?;
        let obsoleted_db = redb::Database::open(image.obsoleted_db_path())
            .map_err(|e| SolverError::new(format!("open obsoleted db: {}", e)))?;
        let obsoleted_tx = obsoleted_db
            .begin_read()
            .map_err(|e| SolverError::new(format!("begin read obsoleted db: {}", e)))?;

        let mut prov = IpsProvider {
            image,
            _catalog_db: catalog_db,
            catalog_tx,
            _obsoleted_db: obsoleted_db,
            _obsoleted_tx: obsoleted_tx,
            names: Mapping::default(),
            name_by_str: BTreeMap::new(),
            strings: Mapping::default(),
            solvables: Mapping::default(),
            cands_by_name: HashMap::new(),
            version_sets: RefCell::new(Mapping::default()),
            vs_name: RefCell::new(Mapping::default()),
            unions: RefCell::new(Mapping::default()),
            publisher_prefs: RefCell::new(HashMap::new()),
        };
        prov.build_index()?;
        Ok(prov)
    }

    fn build_index(&mut self) -> Result<(), SolverError> {
        use crate::image::catalog::CATALOG_TABLE;
        // Iterate catalog table and build in-memory index of non-obsolete candidates
        let table = self
            .catalog_tx
            .open_table(CATALOG_TABLE)
            .map_err(|e| SolverError::new(format!("open catalog table: {}", e)))?;

        // Temporary map: stem string -> Vec<Fmri>
        let mut by_stem: BTreeMap<String, Vec<Fmri>> = BTreeMap::new();
        for entry in table
            .iter()
            .map_err(|e| SolverError::new(format!("iterate catalog table: {}", e)))?
        {
            let (k, v) =
                entry.map_err(|e| SolverError::new(format!("read catalog entry: {}", e)))?;
            let key = k.value(); // stem@version

            // Try to decode manifest and extract full FMRI (including publisher)
            let mut pushed = false;
            if let Ok(manifest) = decode_manifest_bytes_local(v.value()) {
                if let Some(attr) = manifest.attributes.iter().find(|a| a.key == "pkg.fmri") {
                    if let Some(fmri_str) = attr.values.first() {
                        if let Ok(mut fmri) = Fmri::parse(fmri_str) {
                            // Ensure publisher is present; if missing/empty, use image default publisher
                            let missing_pub = fmri
                                .publisher
                                .as_deref()
                                .map(|s| s.is_empty())
                                .unwrap_or(true);
                            if missing_pub {
                                if let Ok(defp) = self.image.default_publisher() {
                                    fmri.publisher = Some(defp.name.clone());
                                }
                            }
                            by_stem
                                .entry(fmri.stem().to_string())
                                .or_default()
                                .push(fmri);
                            pushed = true;
                        }
                    }
                }
            }

            // Fallback: derive FMRI from catalog key if we couldn't push from manifest
            if !pushed {
                if let Some((stem, ver_str)) = key.split_once('@') {
                    let ver_obj = crate::fmri::Version::parse(ver_str).ok();
                    // Prefer default publisher if configured; else leave None by constructing and then setting publisher
                    let mut fmri = if let Some(v) = ver_obj.clone() {
                        if let Ok(defp) = self.image.default_publisher() {
                            Fmri::with_publisher(&defp.name, stem, Some(v))
                        } else {
                            Fmri::with_version(stem, v)
                        }
                    } else {
                        // No parsable version; still record a minimal FMRI without version
                        if let Ok(defp) = self.image.default_publisher() {
                            Fmri::with_publisher(&defp.name, stem, None)
                        } else {
                            Fmri::with_publisher("", stem, None)
                        }
                    };
                    // Normalize: empty publisher string -> None
                    if fmri.publisher.as_deref() == Some("") {
                        fmri.publisher = None;
                    }
                    by_stem.entry(stem.to_string()).or_default().push(fmri);
                }
            }
        }

        // Intern and populate solvables per stem
        for (stem, mut fmris) in by_stem {
            let name_id = self.intern_name(&stem);
            // Sort fmris newest-first using IPS ordering
            fmris.sort_by(|a, b| version_order_desc(a, b));
            let mut ids: Vec<SolvableId> = Vec::with_capacity(fmris.len());
            for fmri in fmris {
                let sid = SolvableId(self.solvables.len() as u32);
                self.solvables.insert(
                    sid,
                    PkgCand {
                        id: sid,
                        name_id,
                        fmri,
                    },
                );
                ids.push(sid);
            }
            self.cands_by_name.insert(name_id, ids);
        }
        Ok(())
    }

    fn intern_name(&mut self, name: &str) -> NameId {
        if let Some(id) = self.name_by_str.get(name).copied() {
            return id;
        }
        let id = NameId(self.names.len() as u32);
        self.names.insert(id, name.to_string());
        self.name_by_str.insert(name.to_string(), id);
        id
    }

    fn version_set_for(&self, name: NameId, kind: VersionSetKind) -> VersionSetId {
        let vs_id = VersionSetId(self.version_sets.borrow().len() as u32);
        self.version_sets.borrow_mut().insert(vs_id, kind);
        self.vs_name.borrow_mut().insert(vs_id, name);
        vs_id
    }

    fn lookup_incorporated_release(&self, stem: &str) -> Option<String> {
        if let Ok(table) = self.catalog_tx.open_table(INCORPORATE_TABLE) {
            if let Ok(Some(rel)) = table.get(stem) {
                return Some(String::from_utf8_lossy(rel.value()).to_string());
            }
        }
        None
    }

    fn read_manifest_from_catalog(&self, fmri: &Fmri) -> Option<Manifest> {
        let key = format!("{}@{}", fmri.stem(), fmri.version());
        if let Ok(table) = self.catalog_tx.open_table(CATALOG_TABLE) {
            if let Ok(Some(bytes)) = table.get(key.as_str()) {
                return decode_manifest_bytes_local(bytes.value()).ok();
            }
        }
        None
    }
}

impl<'a> Interner for IpsProvider<'a> {
    fn display_solvable(&self, solvable: SolvableId) -> impl std::fmt::Display + '_ {
        let fmri = &self.solvables.get(solvable).unwrap().fmri;
        fmri.to_string()
    }

    fn display_solvable_name(&self, solvable: SolvableId) -> impl Display + '_ {
        let name_id = self.solvable_name(solvable);
        self.display_name(name_id).to_string()
    }

    fn display_merged_solvables(&self, solvables: &[SolvableId]) -> impl Display + '_ {
        let joined = solvables
            .iter()
            .map(|s| self.display_solvable(*s).to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        joined
    }

    fn display_name(&self, name: NameId) -> impl std::fmt::Display + '_ {
        self.names.get(name).cloned().unwrap_or_default()
    }

    fn display_version_set(&self, version_set: VersionSetId) -> impl std::fmt::Display + '_ {
        match self.version_sets.borrow().get(version_set) {
            Some(VersionSetKind::Any) => "any".to_string(),
            Some(VersionSetKind::ReleaseEq(r)) => format!("release={}", r),
            Some(VersionSetKind::BranchEq(b)) => format!("branch={}", b),
            Some(VersionSetKind::ReleaseAndBranch { release, branch }) => {
                format!("release={}, branch={}", release, branch)
            }
            None => "<unknown>".to_string(),
        }
    }

    fn display_string(&self, string_id: StringId) -> impl std::fmt::Display + '_ {
        self.strings.get(string_id).cloned().unwrap_or_default()
    }

    fn version_set_name(&self, version_set: VersionSetId) -> NameId {
        *self
            .vs_name
            .borrow()
            .get(version_set)
            .expect("version set name present")
    }

    fn solvable_name(&self, solvable: SolvableId) -> NameId {
        self.solvables.get(solvable).unwrap().name_id
    }

    fn version_sets_in_union(
        &self,
        version_set_union: VersionSetUnionId,
    ) -> impl Iterator<Item = VersionSetId> {
        self.unions
            .borrow()
            .get(version_set_union)
            .cloned()
            .unwrap_or_default()
            .into_iter()
    }

    fn resolve_condition(&self, condition: ConditionId) -> Condition {
        // Interpret ConditionId as referencing a VersionSetId directly.
        // This supports simple conditions of the form "requirement holds if
        // version set X is selected". Complex boolean conditions are not
        // generated by this provider at present.
        Condition::Requirement(VersionSetId(condition.as_u32()))
    }
}

// Helper to evaluate if a candidate FMRI matches a VersionSetKind constraint
fn fmri_matches_version_set(fmri: &Fmri, kind: &VersionSetKind) -> bool {
    // Allow composite releases like "20,5.11": a requirement of single token (e.g., "5.11")
    // matches any candidate whose comma-separated release segments contain that token.
    // Multi-token requirements (contain a comma) require exact equality.
    fn release_satisfies(req: &str, cand: &str) -> bool {
        if req == cand {
            return true;
        }
        if req.contains(',') {
            // Multi-token requirement must match exactly
            return false;
        }
        // Single token requirement: match if present among candidate segments
        cand.split(',').any(|seg| seg.trim() == req)
    }
    match kind {
        VersionSetKind::Any => true,
        VersionSetKind::ReleaseEq(req_rel) => fmri
            .version
            .as_ref()
            .map(|v| release_satisfies(req_rel, &v.release) || v.branch.as_deref() == Some(req_rel))
            .unwrap_or(false),
        VersionSetKind::BranchEq(req_branch) => fmri
            .version
            .as_ref()
            .and_then(|v| v.branch.as_ref())
            .map(|b| b == req_branch)
            .unwrap_or(false),
        VersionSetKind::ReleaseAndBranch { release, branch } => {
            let (mut ok_rel, mut ok_branch) = (false, false);
            if let Some(v) = fmri.version.as_ref() {
                ok_rel =
                    release_satisfies(release, &v.release) || v.branch.as_deref() == Some(release);
                ok_branch = v.branch.as_ref().map(|b| b == branch).unwrap_or(false);
            }
            ok_rel && ok_branch
        }
    }
}

#[allow(clippy::too_many_arguments)]
impl<'a> DependencyProvider for IpsProvider<'a> {
    async fn filter_candidates(
        &self,
        candidates: &[SolvableId],
        version_set: VersionSetId,
        inverse: bool,
    ) -> Vec<SolvableId> {
        // If an incorporation lock exists for this name, we intentionally ignore
        // the incoming version_set constraint so that incorporation can override
        // transitive dependency version requirements. The base candidate set
        // returned by get_candidates is already restricted to the locked version(s).
        let name = self.version_set_name(version_set);
        let stem = self.display_name(name).to_string();
        if self.lookup_incorporated_release(&stem).is_some() {
            // Treat all candidates as matching the requirement; the solver's inverse
            // queries should see an empty set to avoid excluding the locked candidate.
            return if inverse { vec![] } else { candidates.to_vec() };
        }

        let kind = self
            .version_sets
            .borrow()
            .get(version_set)
            .cloned()
            .unwrap_or(VersionSetKind::Any);
        candidates
            .iter()
            .copied()
            .filter(|sid| {
                let fmri = &self.solvables.get(*sid).unwrap().fmri;
                let m = fmri_matches_version_set(fmri, &kind);
                if inverse { !m } else { m }
            })
            .collect()
    }

    async fn get_candidates(&self, name: NameId) -> Option<Candidates> {
        let list = self.cands_by_name.get(&name)?;
        // Check if an incorporation lock exists for this stem; if so, restrict candidates
        let stem = self.display_name(name).to_string();
        if let Some(locked_ver) = self.lookup_incorporated_release(&stem) {
            let parsed_lock = crate::fmri::Version::parse(&locked_ver).ok();
            let locked_cands: Vec<SolvableId> = list
                .iter()
                .copied()
                .filter(|sid| {
                    let fmri = &self.solvables.get(*sid).unwrap().fmri;
                    if let Some(cv) = fmri.version.as_ref() {
                        if let Some(lv) = parsed_lock.as_ref() {
                            if cv.release != lv.release {
                                return false;
                            }
                            if cv.branch != lv.branch {
                                return false;
                            }
                            if cv.build != lv.build {
                                return false;
                            }
                            if lv.timestamp.is_some() {
                                return cv.timestamp == lv.timestamp;
                            }
                            true
                        } else {
                            fmri.version() == locked_ver
                        }
                    } else {
                        false
                    }
                })
                .collect();
            if !locked_cands.is_empty() {
                return Some(Candidates {
                    candidates: locked_cands,
                    favored: None,
                    locked: None,
                    hint_dependencies_available: HintDependenciesAvailable::None,
                    excluded: vec![],
                });
            }
        }
        Some(Candidates {
            candidates: list.clone(),
            favored: None,
            locked: None,
            hint_dependencies_available: HintDependenciesAvailable::None,
            excluded: vec![],
        })
    }

    async fn sort_candidates(&self, _solver: &SolverCache<Self>, solvables: &mut [SolvableId]) {
        // Determine publisher preference order for this name
        let name_id = if solvables.is_empty() {
            return;
        } else {
            self.solvable_name(solvables[0])
        };
        let prefs_opt = self.publisher_prefs.borrow().get(&name_id).cloned();
        let pub_order = prefs_opt.unwrap_or_else(|| build_publisher_preference(None, self.image));

        let idx_of = |pubname: &str| -> usize {
            pub_order
                .iter()
                .position(|p| p == pubname)
                .unwrap_or(usize::MAX)
        };

        solvables.sort_by(|a, b| {
            let fa = &self.solvables.get(*a).unwrap().fmri;
            let fb = &self.solvables.get(*b).unwrap().fmri;
            // First: compare releases only
            let rel_ord = cmp_release_desc(fa, fb);
            if rel_ord != std::cmp::Ordering::Equal {
                return rel_ord;
            }
            // If same release: prefer publisher order
            let ia = fa.publisher.as_deref().map(idx_of).unwrap_or(usize::MAX);
            let ib = fb.publisher.as_deref().map(idx_of).unwrap_or(usize::MAX);
            if ia != ib {
                return ia.cmp(&ib);
            }
            // Same publisher: prefer newest timestamp
            version_order_desc(fa, fb)
        });
    }

    async fn get_dependencies(&self, solvable: SolvableId) -> RDependencies {
        let pkg = self.solvables.get(solvable).unwrap();
        let fmri = &pkg.fmri;
        let manifest_opt = self.read_manifest_from_catalog(fmri);
        let Some(manifest) = manifest_opt else {
            return RDependencies::Known(KnownDependencies::default());
        };

        // Build requirements for "require" deps
        let mut reqs: Vec<ConditionalRequirement> = Vec::new();
        let parent_branch = fmri.version.as_ref().and_then(|v| v.branch.clone());
        let parent_pub = fmri.publisher.as_deref();

        for d in manifest
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == "require")
        {
            if let Some(df) = &d.fmri {
                let stem = df.stem().to_string();
                let Some(child_name_id) = self.name_by_str.get(&stem).copied() else {
                    // If the dependency name isn't present in the catalog index, skip it
                    continue;
                };
                // Create version set by release (from dep expr) and branch (from parent)
                let vs_kind = match (&df.version, &parent_branch) {
                    (Some(ver), Some(branch)) => VersionSetKind::ReleaseAndBranch {
                        release: ver.release.clone(),
                        branch: branch.clone(),
                    },
                    (Some(ver), None) => VersionSetKind::ReleaseEq(ver.release.clone()),
                    (None, Some(branch)) => VersionSetKind::BranchEq(branch.clone()),
                    (None, None) => VersionSetKind::Any,
                };
                let vs_id = self.version_set_for(child_name_id, vs_kind);
                reqs.push(ConditionalRequirement::from(vs_id));

                // Set publisher preferences for the child to parent-first, then image order
                let order = build_publisher_preference(parent_pub, self.image);
                self.publisher_prefs
                    .borrow_mut()
                    .entry(child_name_id)
                    .or_insert(order);
            }
        }
        RDependencies::Known(KnownDependencies {
            requirements: reqs,
            constrains: vec![],
        })
    }
}

#[derive(Debug, Clone)]
pub enum SolverProblemKind {
    NoCandidates {
        stem: String,
        release: Option<String>,
        branch: Option<String>,
    },
    Unsolvable,
}

#[derive(Debug, Clone)]
pub struct SolverFailure {
    pub kind: SolverProblemKind,
    pub roots: Vec<Constraint>,
}

#[derive(Debug, Error, Diagnostic)]
#[error("Solver error: {message}")]
#[diagnostic(
    code(ips::solver_error::generic),
    help(
        "Check package names and repository catalogs. Use 'pkg6 image catalog --dump' for debugging."
    )
)]
pub struct SolverError {
    pub message: String,
    pub problem: Option<SolverFailure>,
}

impl SolverError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            problem: None,
        }
    }
    pub fn with_details(msg: impl Into<String>, problem: SolverFailure) -> Self {
        Self {
            message: msg.into(),
            problem: Some(problem),
        }
    }
    pub fn problem(&self) -> Option<&SolverFailure> {
        self.problem.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPkg {
    pub fmri: Fmri,
    pub manifest: Manifest,
}

#[derive(Debug, Default, Clone)]
pub struct InstallPlan {
    pub add: Vec<ResolvedPkg>,
    pub remove: Vec<ResolvedPkg>,
    pub update: Vec<(ResolvedPkg, ResolvedPkg)>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Constraint {
    pub stem: String,
    // If present, this holds the main release component to match (e.g., "1.18.0" or "5.11").
    // IPS dependency expressions should be matched by their main release, not the full
    // branch/build/timestamp string.
    pub version_req: Option<String>,
    // Preferred publishers in order of priority. When multiple candidates have the same
    // best release and timestamp, we pick the first matching publisher in this list.
    pub preferred_publishers: Vec<String>,
    // When resolving a dependency, enforce staying on the dependant's branch.
    pub branch: Option<String>,
}

/// IPS-specific comparison: newest release first; if equal, newest timestamp.
fn cmp_release_desc(a: &Fmri, b: &Fmri) -> std::cmp::Ordering {
    let a_rel = a.version.as_ref();
    let b_rel = b.version.as_ref();
    match (a_rel, b_rel) {
        (Some(va), Some(vb)) => match (va.release_to_semver(), vb.release_to_semver()) {
            (Ok(ra), Ok(rb)) => ra.cmp(&rb).reverse(),
            _ => va.release.cmp(&vb.release).reverse(),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn version_order_desc(a: &Fmri, b: &Fmri) -> std::cmp::Ordering {
    // Compare by release (semver padded) if possible
    let a_rel = a.version.clone();
    let b_rel = b.version.clone();

    match (&a_rel, &b_rel) {
        (Some(va), Some(vb)) => {
            // Compare release using semver padded via Version::release_to_semver
            let rel_cmp = match (va.release_to_semver(), vb.release_to_semver()) {
                (Ok(ra), Ok(rb)) => ra.cmp(&rb).reverse(),
                _ => va.release.cmp(&vb.release).reverse(),
            };
            if rel_cmp != std::cmp::Ordering::Equal {
                return rel_cmp;
            }
            // Same release: compare timestamp (lexicographic works for YYYYMMDDThhmmssZ)
            match (&va.timestamp, &vb.timestamp) {
                (Some(ta), Some(tb)) => ta.cmp(tb).reverse(),
                (Some(_), None) => std::cmp::Ordering::Less, // Some > None (newer preferred)
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        }
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Resolve an install plan for the given constraints.
pub fn resolve_install(
    image: &Image,
    constraints: &[Constraint],
) -> Result<InstallPlan, SolverError> {
    // Build provider indexed from catalog
    let mut provider = IpsProvider::new(image)?;

    // Construct problem requirements from top-level constraints
    let problem = RProblem::default();

    // Augment publisher preferences for roots and create version sets
    let image_pub_order: Vec<String> = image.publishers().iter().map(|p| p.name.clone()).collect();
    let default_pub = image
        .default_publisher()
        .map(|p| p.name.clone())
        .unwrap_or_else(|_| String::new());

    // Track each root's NameId with the originating constraint for diagnostics
    let mut root_names: Vec<(NameId, Constraint)> = Vec::new();

    let mut reqs: Vec<ConditionalRequirement> = Vec::new();
    for c in constraints.iter().cloned() {
        // Intern name
        let name_id = provider.intern_name(&c.stem);
        root_names.push((name_id, c.clone()));

        // Store publisher preferences for this root
        let mut prefs = c.preferred_publishers.clone();
        if prefs.is_empty() {
            prefs = image_pub_order.clone();
            if !default_pub.is_empty() && !prefs.iter().any(|p| p == &default_pub) {
                prefs.push(default_pub.clone());
            }
        }
        provider.publisher_prefs.borrow_mut().insert(name_id, prefs);

        // Build version set: by release if provided; optionally by branch if present
        let vs_kind = match (c.version_req, c.branch) {
            (Some(release), Some(branch)) => VersionSetKind::ReleaseAndBranch { release, branch },
            (Some(release), None) => VersionSetKind::ReleaseEq(release),
            (None, Some(branch)) => VersionSetKind::BranchEq(branch),
            (None, None) => VersionSetKind::Any,
        };
        let vs_id = provider.version_set_for(name_id, vs_kind);
        reqs.push(ConditionalRequirement::from(vs_id));
    }
    let problem = problem.requirements(reqs);

    // Early diagnostic: detect roots with zero candidates before invoking solver
    let mut missing: Vec<String> = Vec::new();
    for (name_id, c) in &root_names {
        let has = provider
            .cands_by_name
            .get(name_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if !has {
            let mut req = c.stem.clone();
            if let Some(v) = &c.version_req {
                req.push('@');
                req.push_str(v);
            }
            missing.push(req);
        }
    }
    if !missing.is_empty() {
        let pubs: Vec<String> = image.publishers().iter().map(|p| p.name.clone()).collect();
        // Pick the first missing root and its constraint for structured problem
        let mut first_missing: Option<Constraint> = None;
        for (name_id, c) in &root_names {
            let has = provider
                .cands_by_name
                .get(name_id)
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            if !has {
                first_missing = Some(c.clone());
                break;
            }
        }
        let roots: Vec<Constraint> = root_names.iter().map(|(_, c)| c.clone()).collect();
        let problem = if let Some(c) = first_missing {
            SolverFailure {
                kind: SolverProblemKind::NoCandidates {
                    stem: c.stem.clone(),
                    release: c.version_req.clone(),
                    branch: c.branch.clone(),
                },
                roots,
            }
        } else {
            SolverFailure {
                kind: SolverProblemKind::Unsolvable,
                roots,
            }
        };
        return Err(SolverError::with_details(
            format!(
                "No candidates found for requested package(s): {}.\nChecked publishers: {}.\nRun 'pkg6 refresh' to update catalogs or verify the package names.",
                missing.join(", "),
                pubs.join(", ")
            ),
            problem,
        ));
    }

    // Before moving provider into the solver, capture useful snapshots for diagnostics
    let mut sid_to_fmri: HashMap<SolvableId, Fmri> = HashMap::new();
    for ids in provider.cands_by_name.values() {
        for sid in ids {
            let fmri = provider.solvables.get(*sid).unwrap().fmri.clone();
            sid_to_fmri.insert(*sid, fmri);
        }
    }
    // Snapshot: NameId -> name string
    let mut name_to_string: HashMap<NameId, String> = HashMap::new();
    for (name_id, _cands) in provider.cands_by_name.iter() {
        name_to_string.insert(*name_id, provider.display_name(*name_id).to_string());
    }
    // Reverse: stem string -> NameId
    let mut stem_to_nameid: HashMap<String, NameId> = HashMap::new();
    for (nid, nstr) in name_to_string.iter() {
        stem_to_nameid.insert(nstr.clone(), *nid);
    }
    // Snapshot: NameId -> candidate FMRIs
    let mut name_to_fmris: HashMap<NameId, Vec<Fmri>> = HashMap::new();
    for (name_id, sids) in provider.cands_by_name.iter() {
        let mut v: Vec<Fmri> = Vec::new();
        for sid in sids {
            if let Some(pc) = provider.solvables.get(*sid) {
                v.push(pc.fmri.clone());
            }
        }
        name_to_fmris.insert(*name_id, v);
    }
    // Snapshot: Catalog manifest cache keyed by stem@version for all candidates
    let mut key_to_manifest: HashMap<String, Manifest> = HashMap::new();
    for fmris in name_to_fmris.values() {
        for fmri in fmris {
            let key = format!("{}@{}", fmri.stem(), fmri.version());
            if !key_to_manifest.contains_key(&key) {
                if let Some(man) = provider.read_manifest_from_catalog(fmri) {
                    key_to_manifest.insert(key, man);
                }
            }
        }
    }

    // Run the solver
    let roots_for_err: Vec<Constraint> = root_names.iter().map(|(_, c)| c.clone()).collect();
    let mut solver = RSolver::new(provider);
    let solution_ids =
        solver
            .solve(problem)
            .map_err(|conflict_or_cancelled| match conflict_or_cancelled {
                UnsolvableOrCancelled::Unsolvable(u) => {
                    let msg = u.display_user_friendly(&solver).to_string();
                    SolverError::with_details(
                        msg,
                        SolverFailure {
                            kind: SolverProblemKind::Unsolvable,
                            roots: roots_for_err.clone(),
                        },
                    )
                }
                UnsolvableOrCancelled::Cancelled(_) => SolverError::with_details(
                    "dependency resolution cancelled".to_string(),
                    SolverFailure {
                        kind: SolverProblemKind::Unsolvable,
                        roots: roots_for_err.clone(),
                    },
                ),
            })?;

    // Build plan from solution
    let image_ref = image;
    let mut plan = InstallPlan::default();
    for sid in solution_ids {
        if let Some(fmri) = sid_to_fmri.get(&sid).cloned() {
            // Prefer repository manifest; fallback to preloaded catalog snapshot, then image catalog
            let key = format!("{}@{}", fmri.stem(), fmri.version());
            let manifest = match image_ref.get_manifest_from_repository(&fmri) {
                Ok(m) => m,
                Err(repo_err) => {
                    if let Some(m) = key_to_manifest.get(&key).cloned() {
                        m
                    } else {
                        match image_ref.get_manifest_from_catalog(&fmri) {
                            Ok(Some(m)) => m,
                            _ => {
                                return Err(SolverError::new(format!(
                                    "failed to obtain manifest for {}: {}",
                                    fmri, repo_err
                                )));
                            }
                        }
                    }
                }
            };
            plan.reasons.push(format!("selected {} via solver", fmri));
            plan.add.push(ResolvedPkg { fmri, manifest });
        }
    }
    Ok(plan)
}

fn build_publisher_preference(parent_pub: Option<&str>, image: &Image) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    // 1) parent publisher first if provided
    if let Some(p) = parent_pub {
        order.push(p.to_string());
    }
    // 2) image publishers in configured order
    for p in image.publishers() {
        if !order.iter().any(|x| x == &p.name) {
            order.push(p.name.clone());
        }
    }
    // 3) default publisher at the end if missing
    if let Ok(def) = image.default_publisher() {
        if !order.iter().any(|x| x == &def.name) {
            order.push(def.name.clone());
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    // These are light smoke tests using a fabricated Image may be non-trivial.
    // Leave placeholder tests to ensure API compiles.
    #[test]
    fn install_plan_struct_defaults() {
        let plan = InstallPlan::default();
        assert!(plan.add.is_empty());
        assert!(plan.remove.is_empty());
        assert!(plan.update.is_empty());
    }
}

#[cfg(test)]
mod solver_integration_tests {
    use super::*;
    use crate::actions::Dependency;
    use crate::fmri::Version;
    use crate::image::ImageType;
    use crate::image::catalog::{CATALOG_TABLE, OBSOLETED_TABLE};
    use redb::Database;
    use tempfile::tempdir;

    fn mk_version(release: &str, branch: Option<&str>, timestamp: Option<&str>) -> Version {
        let mut v = Version::new(release);
        if let Some(b) = branch {
            v.branch = Some(b.to_string());
        }
        if let Some(t) = timestamp {
            v.timestamp = Some(t.to_string());
        }
        v
    }

    fn mk_fmri(publisher: &str, name: &str, v: Version) -> Fmri {
        Fmri::with_publisher(publisher, name, Some(v))
    }

    fn mk_manifest(fmri: &Fmri, req_deps: &[Fmri]) -> Manifest {
        let mut m = Manifest::new();
        // Add pkg.fmri attribute
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec![fmri.to_string()];
        m.attributes.push(attr);
        // Add require dependencies
        for df in req_deps {
            let mut d = Dependency::default();
            d.fmri = Some(df.clone());
            d.dependency_type = "require".to_string();
            m.dependencies.push(d);
        }
        m
    }

    fn write_manifest_to_catalog(image: &Image, fmri: &Fmri, manifest: &Manifest) {
        let db = Database::open(image.catalog_db_path()).expect("open catalog db");
        let tx = db.begin_write().expect("begin write");
        {
            let mut table = tx.open_table(CATALOG_TABLE).expect("open catalog table");
            let key = format!("{}@{}", fmri.stem(), fmri.version());
            let val = serde_json::to_vec(manifest).expect("serialize manifest");
            table
                .insert(key.as_str(), val.as_slice())
                .expect("insert manifest");
        }
        tx.commit().expect("commit");
    }

    fn mark_obsolete(image: &Image, fmri: &Fmri) {
        let db = Database::open(image.obsoleted_db_path()).expect("open obsoleted db");
        let tx = db.begin_write().expect("begin write");
        {
            let mut table = tx
                .open_table(OBSOLETED_TABLE)
                .expect("open obsoleted table");
            let key = fmri.to_string();
            // store empty value
            let empty: Vec<u8> = Vec::new();
            table
                .insert(key.as_str(), empty.as_slice())
                .expect("insert obsolete");
        }
        tx.commit().expect("commit");
    }

    fn make_image_with_publishers(pubs: &[(&str, bool)]) -> Image {
        let td = tempdir().expect("tempdir");
        // Persist the directory for the duration of the test
        let path = td.keep();
        let mut img = Image::create_image(&path, ImageType::Partial).expect("create image");
        for (name, is_default) in pubs.iter().copied() {
            img.add_publisher(
                name,
                &format!("https://example.com/{name}"),
                vec![],
                is_default,
            )
            .expect("add publisher");
        }
        img
    }

    #[test]
    fn select_newest_release_then_timestamp() {
        let img = make_image_with_publishers(&[("pubA", true)]);

        let fmri_100_old = mk_fmri(
            "pubA",
            "pkg/alpha",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        let fmri_100_new = mk_fmri(
            "pubA",
            "pkg/alpha",
            mk_version("1.0", None, Some("20200201T000000Z")),
        );
        let fmri_110_any = mk_fmri(
            "pubA",
            "pkg/alpha",
            mk_version("1.1", None, Some("20200115T000000Z")),
        );

        write_manifest_to_catalog(&img, &fmri_100_old, &mk_manifest(&fmri_100_old, &[]));
        write_manifest_to_catalog(&img, &fmri_100_new, &mk_manifest(&fmri_100_new, &[]));
        write_manifest_to_catalog(&img, &fmri_110_any, &mk_manifest(&fmri_110_any, &[]));

        let c = Constraint {
            stem: "pkg/alpha".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        assert!(!plan.add.is_empty());
        let chosen = &plan.add[0].fmri;
        assert_eq!(chosen.version.as_ref().unwrap().release, "1.1");
    }

    #[test]
    fn ignore_obsolete_candidates() {
        let img = make_image_with_publishers(&[("pubA", true)]);

        let fmri_non_obsolete = mk_fmri(
            "pubA",
            "pkg/beta",
            mk_version("0.9", None, Some("20200101T000000Z")),
        );
        let fmri_obsolete = mk_fmri(
            "pubA",
            "pkg/beta",
            mk_version("1.0", None, Some("20200301T000000Z")),
        );

        write_manifest_to_catalog(
            &img,
            &fmri_non_obsolete,
            &mk_manifest(&fmri_non_obsolete, &[]),
        );
        // mark the 1.0 as obsolete (not adding to catalog table)
        mark_obsolete(&img, &fmri_obsolete);

        let c = Constraint {
            stem: "pkg/beta".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        assert!(!plan.add.is_empty());
        let chosen = &plan.add[0].fmri;
        assert_eq!(chosen.version.as_ref().unwrap().release, "0.9");
    }

    #[test]
    fn resolve_uses_repo_manifest_after_solving() {
        use crate::image::ImageType;
        use crate::repository::{FileBackend, RepositoryVersion, WritableRepository};
        use std::fs;

        // Create a temp image
        let td_img = tempdir().expect("tempdir img");
        let img_path = td_img.path().to_path_buf();
        let mut img = Image::create_image(&img_path, ImageType::Partial).expect("create image");

        // Create a temp file-based repository and add publisher
        let td_repo = tempdir().expect("tempdir repo");
        let repo_path = td_repo.path().to_path_buf();
        let mut repo = FileBackend::create(&repo_path, RepositoryVersion::V4).expect("create repo");
        repo.add_publisher("pubA").expect("add publisher");

        // Configure image publisher to point to file:// repo
        let origin = format!("file://{}", repo_path.display());
        img.add_publisher("pubA", &origin, vec![], true)
            .expect("add publisher to image");

        // Define FMRI and limited manifest in catalog (deps only)
        let fmri = mk_fmri(
            "pubA",
            "pkg/alpha",
            mk_version("1.0", None, Some("20200401T000000Z")),
        );
        let limited = mk_manifest(&fmri, &[]); // no files/dirs
        write_manifest_to_catalog(&img, &fmri, &limited);

        // Write full manifest into repository at expected path
        let repo_manifest_path =
            FileBackend::construct_manifest_path(&repo_path, "pubA", fmri.stem(), &fmri.version());
        if let Some(parent) = repo_manifest_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let full_manifest_text = format!(
            "set name=pkg.fmri value={}\n\
             dir path=opt/test owner=root group=bin mode=0755\n\
             file path=opt/test/hello owner=root group=bin mode=0644\n",
            fmri
        );
        fs::write(&repo_manifest_path, full_manifest_text).expect("write manifest to repo");

        // Resolve and ensure we got the repo (full) manifest with file/dir actions
        let c = Constraint {
            stem: "pkg/alpha".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        assert_eq!(plan.add.len(), 1);
        let man = &plan.add[0].manifest;
        assert!(
            man.directories.len() >= 1,
            "expected directories from repo manifest"
        );
        assert!(man.files.len() >= 1, "expected files from repo manifest");
    }

    #[test]
    fn dependency_sticks_to_parent_branch() {
        let img = make_image_with_publishers(&[("pubA", true)]);
        // Parent pkg on branch 1 with a require on dep@5.11
        let parent = mk_fmri(
            "pubA",
            "pkg/parent",
            mk_version("5.11", Some("1"), Some("20200102T000000Z")),
        );
        let dep_req = Fmri::with_version("pkg/dep", Version::new("5.11"));
        let parent_manifest = mk_manifest(&parent, &[dep_req.clone()]);
        write_manifest_to_catalog(&img, &parent, &parent_manifest);

        // dep on branch 1 (older) and branch 2 (newer) — branch 1 must be selected
        let dep_branch1_old = mk_fmri(
            "pubA",
            "pkg/dep",
            mk_version("5.11", Some("1"), Some("20200101T000000Z")),
        );
        let dep_branch1_new = mk_fmri(
            "pubA",
            "pkg/dep",
            mk_version("5.11", Some("1"), Some("20200201T000000Z")),
        );
        let dep_branch2_newer = mk_fmri(
            "pubA",
            "pkg/dep",
            mk_version("5.11", Some("2"), Some("20200401T000000Z")),
        );
        write_manifest_to_catalog(&img, &dep_branch1_old, &mk_manifest(&dep_branch1_old, &[]));
        write_manifest_to_catalog(&img, &dep_branch1_new, &mk_manifest(&dep_branch1_new, &[]));
        write_manifest_to_catalog(
            &img,
            &dep_branch2_newer,
            &mk_manifest(&dep_branch2_newer, &[]),
        );

        let c = Constraint {
            stem: "pkg/parent".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        // find dep in plan
        let dep_pkg = plan
            .add
            .iter()
            .find(|p| p.fmri.stem() == "pkg/dep")
            .expect("dep present");
        let v = dep_pkg.fmri.version.as_ref().unwrap();
        assert_eq!(v.release, "5.11");
        assert_eq!(v.branch.as_deref(), Some("1"));
        assert_eq!(v.timestamp.as_deref(), Some("20200201T000000Z"));
    }

    #[test]
    fn dependency_prefers_parent_publisher_over_newer_other_publisher() {
        // Parent is from pubA; dep exists on pubA (older) and pubB (newer). Expect pubA.
        let img = make_image_with_publishers(&[("pubA", true), ("pubB", false)]);
        // Ensure image publishers order contains both; default already set by first.

        let parent = mk_fmri(
            "pubA",
            "pkg/root",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        let dep_req = Fmri::with_version("pkg/child", Version::new("1.0"));
        let parent_manifest = mk_manifest(&parent, &[dep_req.clone()]);
        write_manifest_to_catalog(&img, &parent, &parent_manifest);

        let dep_pub_a_old = mk_fmri(
            "pubA",
            "pkg/child",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        let dep_pub_b_new = mk_fmri(
            "pubB",
            "pkg/child",
            mk_version("1.0", None, Some("20200301T000000Z")),
        );
        write_manifest_to_catalog(&img, &dep_pub_a_old, &mk_manifest(&dep_pub_a_old, &[]));
        write_manifest_to_catalog(&img, &dep_pub_b_new, &mk_manifest(&dep_pub_b_new, &[]));

        let c = Constraint {
            stem: "pkg/root".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        let dep_pkg = plan
            .add
            .iter()
            .find(|p| p.fmri.stem() == "pkg/child")
            .expect("child present");
        assert_eq!(dep_pkg.fmri.publisher.as_deref(), Some("pubA"));
    }

    #[test]
    fn top_level_release_only_version_requirement() {
        let img = make_image_with_publishers(&[("pubA", true)]);
        let v10_old = mk_fmri(
            "pubA",
            "pkg/vers",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        let v10_new = mk_fmri(
            "pubA",
            "pkg/vers",
            mk_version("1.0", None, Some("20200201T000000Z")),
        );
        let v11 = mk_fmri(
            "pubA",
            "pkg/vers",
            mk_version("1.1", None, Some("20200301T000000Z")),
        );
        write_manifest_to_catalog(&img, &v10_old, &mk_manifest(&v10_old, &[]));
        write_manifest_to_catalog(&img, &v10_new, &mk_manifest(&v10_new, &[]));
        write_manifest_to_catalog(&img, &v11, &mk_manifest(&v11, &[]));

        let c = Constraint {
            stem: "pkg/vers".to_string(),
            version_req: Some("1.0".to_string()),
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        let chosen = &plan.add[0].fmri;
        let v = chosen.version.as_ref().unwrap();
        assert_eq!(v.release, "1.0");
        assert_eq!(v.timestamp.as_deref(), Some("20200201T000000Z"));
    }
}

#[cfg(test)]
mod no_candidate_error_tests {
    use super::*;
    use crate::image::ImageType;

    #[test]
    fn error_message_includes_no_candidates() {
        // Create a temporary image with a publisher but no packages
        let td = tempfile::tempdir().expect("tempdir");
        let img_path = td.path().to_path_buf();
        let mut img = Image::create_image(&img_path, ImageType::Partial).expect("create image");
        img.add_publisher("pubA", "https://example.com/pubA", vec![], true)
            .expect("add publisher");

        // Request a non-existent package so the root has zero candidates
        let c = Constraint {
            stem: "pkg/does-not-exist".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let err = resolve_install(&img, &[c]).err().expect("expected error");
        let msg = err.message;
        assert!(
            msg.contains("No candidates") || msg.contains("no candidates"),
            "unexpected message: {}",
            msg
        );
    }
}

#[cfg(test)]
mod solver_error_message_tests {
    use super::*;
    use crate::actions::{Dependency, Manifest};
    use crate::fmri::{Fmri, Version};
    use crate::image::ImageType;
    use crate::image::catalog::CATALOG_TABLE;
    use redb::Database;

    fn mk_version(release: &str, branch: Option<&str>, timestamp: Option<&str>) -> Version {
        let mut v = Version::new(release);
        if let Some(b) = branch {
            v.branch = Some(b.to_string());
        }
        if let Some(t) = timestamp {
            v.timestamp = Some(t.to_string());
        }
        v
    }

    fn mk_fmri(publisher: &str, name: &str, v: Version) -> Fmri {
        Fmri::with_publisher(publisher, name, Some(v))
    }

    fn mk_manifest_with_dep(parent: &Fmri, dep: &Fmri) -> Manifest {
        let mut m = Manifest::new();
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec![parent.to_string()];
        m.attributes.push(attr);
        let mut d = Dependency::default();
        d.fmri = Some(dep.clone());
        d.dependency_type = "require".to_string();
        m.dependencies.push(d);
        m
    }

    fn write_manifest_to_catalog(image: &Image, fmri: &Fmri, manifest: &Manifest) {
        let db = Database::open(image.catalog_db_path()).expect("open catalog db");
        let tx = db.begin_write().expect("begin write");
        {
            let mut table = tx.open_table(CATALOG_TABLE).expect("open catalog table");
            let key = format!("{}@{}", fmri.stem(), fmri.version());
            let val = serde_json::to_vec(manifest).expect("serialize manifest");
            table
                .insert(key.as_str(), val.as_slice())
                .expect("insert manifest");
        }
        tx.commit().expect("commit");
    }

    #[test]
    fn unsatisfied_dependency_message_no_clause_ids() {
        let td = tempfile::tempdir().expect("tempdir");
        let img_path = td.path().to_path_buf();
        let mut img = Image::create_image(&img_path, ImageType::Partial).expect("create image");
        img.add_publisher("pubA", "https://example.com/pubA", vec![], true)
            .expect("add publisher");

        // Parent requires child@2.0 but only child@1.0 exists in catalog (unsatisfiable)
        let parent = mk_fmri(
            "pubA",
            "pkg/root",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        let child_req = Fmri::with_version("pkg/child", Version::new("2.0"));
        let parent_manifest = mk_manifest_with_dep(&parent, &child_req);
        write_manifest_to_catalog(&img, &parent, &parent_manifest);
        // Add a child candidate with non-matching release
        let child_present = mk_fmri(
            "pubA",
            "pkg/child",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        write_manifest_to_catalog(&img, &child_present, &Manifest::new());

        let c = Constraint {
            stem: "pkg/root".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let err = resolve_install(&img, &[c])
            .err()
            .expect("expected solver error");
        let msg = err.message;
        let lower = msg.to_lowercase();
        assert!(
            !lower.contains("clauseid("),
            "message should not include ClauseId identifiers: {}",
            msg
        );
        assert!(
            lower.contains("cannot be installed") || lower.contains("rejected because"),
            "expected a clear rejection explanation in message: {}",
            msg
        );
        assert!(
            lower.contains("unsatisfied dependency") || lower.contains("no candidates"),
            "expected explanation about missing candidates or unsatisfied dependency in message: {}",
            msg
        );
    }
}

#[cfg(test)]
mod incorporate_lock_tests {
    use super::*;
    use crate::actions::Dependency;
    use crate::fmri::Version;
    use crate::image::ImageType;
    use crate::image::catalog::CATALOG_TABLE;
    use redb::Database;
    use tempfile::tempdir;

    fn mk_version(release: &str, branch: Option<&str>, timestamp: Option<&str>) -> Version {
        let mut v = Version::new(release);
        if let Some(b) = branch {
            v.branch = Some(b.to_string());
        }
        if let Some(t) = timestamp {
            v.timestamp = Some(t.to_string());
        }
        v
    }

    fn mk_fmri(publisher: &str, name: &str, v: Version) -> Fmri {
        Fmri::with_publisher(publisher, name, Some(v))
    }

    fn write_manifest_to_catalog(image: &Image, fmri: &Fmri, manifest: &Manifest) {
        let db = Database::open(image.catalog_db_path()).expect("open catalog db");
        let tx = db.begin_write().expect("begin write");
        {
            let mut table = tx.open_table(CATALOG_TABLE).expect("open catalog table");
            let key = format!("{}@{}", fmri.stem(), fmri.version());
            let val = serde_json::to_vec(manifest).expect("serialize manifest");
            table
                .insert(key.as_str(), val.as_slice())
                .expect("insert manifest");
        }
        tx.commit().expect("commit");
    }

    fn make_image_with_publishers(pubs: &[(&str, bool)]) -> Image {
        let td = tempdir().expect("tempdir");
        let path = td.keep();
        let mut img = Image::create_image(&path, ImageType::Partial).expect("create image");
        for (name, is_default) in pubs.iter().copied() {
            img.add_publisher(
                name,
                &format!("https://example.com/{name}"),
                vec![],
                is_default,
            )
            .expect("add publisher");
        }
        img
    }

    #[test]
    fn incorporate_lock_enforced() {
        let img = make_image_with_publishers(&[("pubA", true)]);
        // Two versions of same stem in catalog
        let v_old = mk_fmri(
            "pubA",
            "compress/gzip",
            mk_version("1.0.0", None, Some("20200101T000000Z")),
        );
        let v_new = mk_fmri(
            "pubA",
            "compress/gzip",
            mk_version("2.0.0", None, Some("20200201T000000Z")),
        );
        write_manifest_to_catalog(&img, &v_old, &Manifest::new());
        write_manifest_to_catalog(&img, &v_new, &Manifest::new());

        // Add incorporation lock to old version
        img.add_incorporation_lock("compress/gzip", &v_old.version())
            .expect("add lock");

        // Resolve without version constraints should pick locked version
        let c = Constraint {
            stem: "compress/gzip".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        assert_eq!(plan.add.len(), 1);
        assert_eq!(plan.add[0].fmri.version(), v_old.version());
    }

    #[test]
    fn incorporate_lock_ignored_if_missing() {
        let img = make_image_with_publishers(&[("pubA", true)]);
        // Only version 2.0 exists
        let v_new = mk_fmri(
            "pubA",
            "compress/gzip",
            mk_version("2.0.0", None, Some("20200201T000000Z")),
        );
        write_manifest_to_catalog(&img, &v_new, &Manifest::new());
        // Add lock to non-existent 1.0.0 -> should be ignored
        img.add_incorporation_lock("compress/gzip", "1.0.0")
            .expect("add lock");
        let c = Constraint {
            stem: "compress/gzip".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");
        assert_eq!(plan.add.len(), 1);
        assert_eq!(plan.add[0].fmri.version(), v_new.version());
    }

    #[test]
    fn incorporation_overrides_transitive_requirement() {
        let img = make_image_with_publishers(&[("pubA", true)]);
        // Build package chain: gzip -> system/library -> system/library/mozilla-nss -> database/sqlite-3@3.46
        let gzip = mk_fmri(
            "pubA",
            "compress/gzip",
            mk_version("1.14", None, Some("20250411T052732Z")),
        );
        let slib = mk_fmri(
            "pubA",
            "system/library",
            mk_version("0.5.11", None, Some("20240101T000000Z")),
        );
        let nss = mk_fmri(
            "pubA",
            "system/library/mozilla-nss",
            mk_version("3.98", None, Some("20240102T000000Z")),
        );

        // sqlite candidates
        let sqlite_old = mk_fmri("pubA", "database/sqlite-3", Version::new("3.46"));
        let sqlite_new = mk_fmri(
            "pubA",
            "database/sqlite-3",
            Version::parse("3.50.4-2025.0.0.0").unwrap(),
        );

        // gzip requires system/library (no version)
        let mut man_gzip = Manifest::new();
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec![gzip.to_string()];
        man_gzip.attributes.push(attr);
        let mut d = Dependency::default();
        d.fmri = Some(Fmri::with_publisher("pubA", "system/library", None));
        d.dependency_type = "require".to_string();
        man_gzip.dependencies.push(d);
        write_manifest_to_catalog(&img, &gzip, &man_gzip);

        // system/library requires mozilla-nss (no version)
        let mut man_slib = Manifest::new();
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec![slib.to_string()];
        man_slib.attributes.push(attr);
        let mut d = Dependency::default();
        d.fmri = Some(Fmri::with_publisher(
            "pubA",
            "system/library/mozilla-nss",
            None,
        ));
        d.dependency_type = "require".to_string();
        man_slib.dependencies.push(d);
        write_manifest_to_catalog(&img, &slib, &man_slib);

        // mozilla-nss requires sqlite-3@3.46
        let mut man_nss = Manifest::new();
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec![nss.to_string()];
        man_nss.attributes.push(attr);
        let mut d = Dependency::default();
        d.fmri = Some(Fmri::with_version(
            "database/sqlite-3",
            Version::new("3.46"),
        ));
        d.dependency_type = "require".to_string();
        man_nss.dependencies.push(d);
        write_manifest_to_catalog(&img, &nss, &man_nss);

        // Add sqlite candidates to catalog (empty manifests)
        write_manifest_to_catalog(&img, &sqlite_old, &Manifest::new());
        write_manifest_to_catalog(&img, &sqlite_new, &Manifest::new());

        // Add incorporation lock to newer sqlite
        img.add_incorporation_lock("database/sqlite-3", &sqlite_new.version())
            .expect("add sqlite lock");

        // Resolve from top-level gzip; expect sqlite_new to be chosen, overriding 3.46 requirement
        let c = Constraint {
            stem: "compress/gzip".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");

        let picked_sqlite = plan
            .add
            .iter()
            .find(|p| p.fmri.stem() == "database/sqlite-3")
            .expect("sqlite present");
        let v = picked_sqlite.fmri.version.as_ref().unwrap();
        assert_eq!(v.release, "3.50.4");
        assert_eq!(v.build.as_deref(), Some("2025.0.0.0"));
    }
}

#[cfg(test)]
mod composite_release_tests {
    use super::*;
    use crate::actions::{Dependency, Manifest};
    use crate::fmri::{Fmri, Version};
    use crate::image::ImageType;
    use crate::image::catalog::CATALOG_TABLE;
    use redb::Database;

    fn mk_version(release: &str, branch: Option<&str>, timestamp: Option<&str>) -> Version {
        let mut v = Version::new(release);
        if let Some(b) = branch {
            v.branch = Some(b.to_string());
        }
        if let Some(t) = timestamp {
            v.timestamp = Some(t.to_string());
        }
        v
    }

    fn mk_fmri(publisher: &str, name: &str, v: Version) -> Fmri {
        Fmri::with_publisher(publisher, name, Some(v))
    }

    fn write_manifest_to_catalog(image: &Image, fmri: &Fmri, manifest: &Manifest) {
        let db = Database::open(image.catalog_db_path()).expect("open catalog db");
        let tx = db.begin_write().expect("begin write");
        {
            let mut table = tx.open_table(CATALOG_TABLE).expect("open catalog table");
            let key = format!("{}@{}", fmri.stem(), fmri.version());
            let val = serde_json::to_vec(manifest).expect("serialize manifest");
            table
                .insert(key.as_str(), val.as_slice())
                .expect("insert manifest");
        }
        tx.commit().expect("commit");
    }

    fn make_image_with_publishers(pubs: &[(&str, bool)]) -> Image {
        let td = tempfile::tempdir().expect("tempdir");
        // Persist the directory for the duration of the test
        let path = td.keep();
        let mut img = Image::create_image(&path, ImageType::Partial).expect("create image");
        for (name, is_default) in pubs.iter().copied() {
            img.add_publisher(
                name,
                &format!("https://example.com/{name}"),
                vec![],
                is_default,
            )
            .expect("add publisher");
        }
        img
    }

    #[test]
    fn require_5_11_matches_candidate_20_5_11() {
        let img = make_image_with_publishers(&[("pubA", true)]);

        // Parent requires child@5.11
        let parent = mk_fmri(
            "pubA",
            "pkg/root",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        let child_req = Fmri::with_version("pkg/child", Version::new("5.11"));
        let mut man = Manifest::new();
        // add pkg.fmri attribute
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec![parent.to_string()];
        man.attributes.push(attr);
        // add require dep
        let mut d = Dependency::default();
        d.fmri = Some(child_req);
        d.dependency_type = "require".to_string();
        man.dependencies.push(d);
        write_manifest_to_catalog(&img, &parent, &man);

        // Only child candidate is release "20,5.11"
        let child = mk_fmri(
            "pubA",
            "pkg/child",
            mk_version("20,5.11", None, Some("20200401T000000Z")),
        );
        write_manifest_to_catalog(&img, &child, &Manifest::new());

        let c = Constraint {
            stem: "pkg/root".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan =
            resolve_install(&img, &[c]).expect("should resolve by matching composite release");
        let dep_pkg = plan
            .add
            .iter()
            .find(|p| p.fmri.stem() == "pkg/child")
            .expect("child present");
        let v = dep_pkg.fmri.version.as_ref().unwrap();
        assert_eq!(v.release, "20");
        assert_eq!(v.branch.as_deref(), Some("5.11"));
    }

    #[test]
    fn require_20_5_11_does_not_match_candidate_5_11() {
        let img = make_image_with_publishers(&[("pubA", true)]);

        // Only candidate for stem is 5.11
        let only = mk_fmri(
            "pubA",
            "pkg/alpha",
            mk_version("5.11", None, Some("20200101T000000Z")),
        );
        write_manifest_to_catalog(&img, &only, &Manifest::new());

        // Top-level constraint requires composite 20,5.11
        let c = Constraint {
            stem: "pkg/alpha".to_string(),
            version_req: Some("20,5.11".to_string()),
            preferred_publishers: vec![],
            branch: None,
        };
        let err = resolve_install(&img, &[c])
            .err()
            .expect("expected unsatisfiable");
        assert!(
            err.message.contains("No candidates")
                || err.message.contains("dependency solving failed")
        );
    }
}

#[cfg(test)]
mod circular_dependency_tests {
    use super::*;
    use crate::actions::Dependency;
    use crate::fmri::{Fmri, Version};
    use crate::image::ImageType;
    use crate::image::catalog::CATALOG_TABLE;
    use redb::Database;
    use std::collections::HashSet;

    fn mk_version(release: &str, branch: Option<&str>, timestamp: Option<&str>) -> Version {
        let mut v = Version::new(release);
        if let Some(b) = branch {
            v.branch = Some(b.to_string());
        }
        if let Some(t) = timestamp {
            v.timestamp = Some(t.to_string());
        }
        v
    }

    fn mk_fmri(publisher: &str, name: &str, v: Version) -> Fmri {
        Fmri::with_publisher(publisher, name, Some(v))
    }

    fn mk_manifest_with_reqs(parent: &Fmri, reqs: &[Fmri]) -> Manifest {
        let mut m = Manifest::new();
        // pkg.fmri attribute
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec![parent.to_string()];
        m.attributes.push(attr);
        // require dependencies
        for df in reqs {
            let mut d = Dependency::default();
            d.fmri = Some(df.clone());
            d.dependency_type = "require".to_string();
            m.dependencies.push(d);
        }
        m
    }

    fn write_manifest_to_catalog(image: &Image, fmri: &Fmri, manifest: &Manifest) {
        let db = Database::open(image.catalog_db_path()).expect("open catalog db");
        let tx = db.begin_write().expect("begin write");
        {
            let mut table = tx.open_table(CATALOG_TABLE).expect("open catalog table");
            let key = format!("{}@{}", fmri.stem(), fmri.version());
            let val = serde_json::to_vec(manifest).expect("serialize manifest");
            table
                .insert(key.as_str(), val.as_slice())
                .expect("insert manifest");
        }
        tx.commit().expect("commit");
    }

    fn make_image_with_publishers(pubs: &[(&str, bool)]) -> Image {
        let td = tempfile::tempdir().expect("tempdir");
        // Persist the directory for the duration of the test
        let path = td.keep();
        let mut img = Image::create_image(&path, ImageType::Partial).expect("create image");
        for (name, is_default) in pubs.iter().copied() {
            img.add_publisher(
                name,
                &format!("https://example.com/{name}"),
                vec![],
                is_default,
            )
            .expect("add publisher");
        }
        img
    }

    #[test]
    fn two_node_cycle_resolves_once_each() {
        let img = make_image_with_publishers(&[("pubA", true)]);

        let a = mk_fmri(
            "pubA",
            "pkg/a",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );
        let b = mk_fmri(
            "pubA",
            "pkg/b",
            mk_version("1.0", None, Some("20200101T000000Z")),
        );

        let a_req_b = Fmri::with_version("pkg/b", Version::new("1.0"));
        let b_req_a = Fmri::with_version("pkg/a", Version::new("1.0"));

        let man_a = mk_manifest_with_reqs(&a, &[a_req_b]);
        let man_b = mk_manifest_with_reqs(&b, &[b_req_a]);

        write_manifest_to_catalog(&img, &a, &man_a);
        write_manifest_to_catalog(&img, &b, &man_b);

        let c = Constraint {
            stem: "pkg/a".to_string(),
            version_req: None,
            preferred_publishers: vec![],
            branch: None,
        };
        let plan = resolve_install(&img, &[c]).expect("resolve");

        // Ensure both packages are present and no duplicates
        let stems: HashSet<String> = plan.add.iter().map(|p| p.fmri.stem().to_string()).collect();
        assert_eq!(stems.len(), plan.add.len(), "no duplicates in plan");
        assert!(stems.contains("pkg/a"));
        assert!(stems.contains("pkg/b"));
    }
}
