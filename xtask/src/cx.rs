//! Workspace context helpers (cargo metadata).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use semver::Version;
use serde::Deserialize;

use crate::changes;

/// A single package relevant to releasing.
#[derive(Debug)]
pub struct Package {
    pub name: String,
    pub version: Version,
    /// Directory of the package (parent of its manifest).
    pub dir: PathBuf,
}

/// Loads all workspace packages via `cargo metadata`.
pub fn load_packages(root: &Path) -> Result<BTreeMap<String, Package>> {
    let out = std::process::Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("run cargo metadata")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("cargo metadata failed:\n{stderr}");
    }
    let meta: Metadata = serde_json::from_slice(&out.stdout).context("parse cargo metadata")?;

    let mut map = BTreeMap::new();
    for pkg in meta.packages {
        let dir = pkg
            .manifest_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow::anyhow!("missing parent for {}", pkg.manifest_path.display()))?;
        map.insert(
            pkg.name.clone(),
            Package {
                name: pkg.name,
                version: Version::parse(&pkg.version)?,
                dir,
            },
        );
    }
    Ok(map)
}

/// The releasable crates, in a stable display order.
pub fn releasable(packages: &BTreeMap<String, Package>) -> Vec<&Package> {
    ["upone", "upone-core", "upone-providers"]
        .iter()
        .filter_map(|name| packages.get(*name))
        .collect()
}

/// Normalizes a note's crate alias to a known package name.
pub fn known_package(packages: &BTreeMap<String, Package>, alias: &str) -> Result<String> {
    let name = changes::resolve_package(alias)?;
    if !packages.contains_key(&name) {
        anyhow::bail!("package not found in workspace: {name}");
    }
    Ok(name)
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<PackageMeta>,
}

#[derive(Deserialize)]
struct PackageMeta {
    name: String,
    version: String,
    manifest_path: PathBuf,
}
