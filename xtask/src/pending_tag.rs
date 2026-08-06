//! `pending-release-tag` — print the version to release if one is pending.

#![allow(clippy::print_stdout)]

use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::{cx, workspace_root};

/// Prints the current `upone` version only if it is higher than the newest
/// existing `vX.Y.Z` git tag (i.e. a release is pending). Prints nothing otherwise.
pub fn run() -> Result<()> {
    let root = workspace_root()?;
    let packages = cx::load_packages(&root)?;
    let upone = packages.get("upone").context("upone package missing")?;

    let latest_tag = latest_release_tag(&root)?;
    let pending = latest_tag.is_none_or(|tag| upone.version > tag);

    if pending {
        println!("{}", upone.version);
    }
    Ok(())
}

/// Returns the highest `vX.Y.Z` tag in the repo, if any.
fn latest_release_tag(root: &std::path::Path) -> Result<Option<semver::Version>> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["tag", "--list", "v[0-9]*"])
        .output()
        .context("run `git tag --list`")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("git tag failed:\n{stderr}");
    }
    let mut best: Option<semver::Version> = None;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let t = line.trim();
        let Ok(version) = semver::Version::parse(t.trim_start_matches('v')) else {
            continue;
        };
        if best.as_ref().is_none_or(|b| version > *b) {
            best = Some(version);
        }
    }
    Ok(best)
}
