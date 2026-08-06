//! Workspace discovery: expands bun/npm/pnpm workspaces into package dirs.
//!
//! Upone detects at the project root and at every workspace package, so a
//! monorepo where `drizzle.config.ts` lives under `packages/db` is still
//! recognized. This module only finds the package dirs; providers detect
//! inside each one.

use std::path::{Path, PathBuf};

/// Returns the workspace package directories below `root` (excluding root
/// itself). Returns an empty vec when the project is not a workspace.
pub fn package_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(globs) = package_json_workspaces(root) {
        collect_globs(root, &globs, &mut dirs);
    }
    if let Some(globs) = pnpm_workspaces(root) {
        collect_globs(root, &globs, &mut dirs);
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Returns true when `root` declares any workspace layout.
pub fn is_workspace(root: &Path) -> bool {
    !package_dirs(root).is_empty()
}

fn collect_globs(root: &Path, globs: &[String], out: &mut Vec<PathBuf>) {
    for glob in globs {
        if let Some(base) = glob.strip_suffix("/*") {
            let base = root.join(base);
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        out.push(path);
                    }
                }
            }
        } else if root.join(glob).is_dir() {
            out.push(root.join(glob));
        }
    }
}

/// Reads `workspaces` from package.json (bun/npm): either `"workspaces":
/// ["apps/*", ...]` or `"workspaces": { "packages": [...] }`.
fn package_json_workspaces(root: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(root.join("package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let workspaces = value.get("workspaces")?;
    let globs = match workspaces {
        serde_json::Value::Array(list) => list
            .iter()
            .filter_map(|g| g.as_str().map(String::from))
            .collect(),
        serde_json::Value::Object(map) => map
            .get("packages")
            .and_then(|p| p.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|g| g.as_str().map(String::from))
                    .collect()
            })?,
        _ => Vec::new(),
    };
    if globs.is_empty() {
        None
    } else {
        Some(globs)
    }
}

/// Reads `packages` from pnpm-workspace.yaml (list of `- <glob>` entries).
fn pnpm_workspaces(root: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(root.join("pnpm-workspace.yaml")).ok()?;
    let globs: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix('-')?;
            let glob = rest.trim().trim_matches(['\'', '"']);
            if glob.is_empty() {
                None
            } else {
                Some(glob.to_string())
            }
        })
        .collect();
    if globs.is_empty() {
        None
    } else {
        Some(globs)
    }
}
