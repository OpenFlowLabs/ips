use std::path::Path;

use crate::actions::executors::{apply_manifest, ApplyOptions, InstallerError};
use crate::actions::Manifest;
use crate::solver::InstallPlan;

/// ActionPlan represents a merged list of actions across all manifests
/// that are to be installed together. It intentionally does not preserve
/// per-package boundaries; executors will run with proper ordering.
#[derive(Debug, Default, Clone)]
pub struct ActionPlan {
    pub manifest: Manifest,
}

impl ActionPlan {
    /// Build an ActionPlan by merging all actions from the install plan's add set.
    /// Note: For now, only directory, file, and link actions are merged for execution.
    pub fn from_install_plan(plan: &InstallPlan) -> Self {
        // Merge all actions from the manifests in plan.add
        let mut merged = Manifest::new();
        for rp in &plan.add {
            // directories
            for d in &rp.manifest.directories {
                merged.directories.push(d.clone());
            }
            // files
            for f in &rp.manifest.files {
                merged.files.push(f.clone());
            }
            // links
            for l in &rp.manifest.links {
                merged.links.push(l.clone());
            }
            // In the future we can merge other action kinds as executor support is added.
        }
        Self { manifest: merged }
    }

    /// Execute the action plan using the executors relative to the provided image root.
    pub fn apply(&self, image_root: &Path, opts: &ApplyOptions) -> Result<(), InstallerError> {
        apply_manifest(image_root, &self.manifest, opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::InstallPlan as SInstallPlan;

    #[test]
    fn build_and_apply_empty_plan_dry_run() {
        // Empty install plan should produce empty action plan and apply should be no-op.
        let plan = SInstallPlan { add: vec![], remove: vec![], update: vec![], reasons: vec![] };
        let ap = ActionPlan::from_install_plan(&plan);
        assert!(ap.manifest.directories.is_empty());
        assert!(ap.manifest.files.is_empty());
        assert!(ap.manifest.links.is_empty());
        let opts = ApplyOptions { dry_run: true };
        let root = Path::new("/tmp/ips_image_test_nonexistent_root");
        // Even if root doesn't exist, dry_run should not perform any IO and succeed.
        let res = ap.apply(root, &opts);
        assert!(res.is_ok());
    }
}
