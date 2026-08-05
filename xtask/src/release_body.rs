//! Rendering and publishing GitHub release bodies.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::{changelog, cx, workspace_root};
use cx::Package;

/// Renders the release body for `version` from the per-crate changelogs.
pub fn render(packages: &BTreeMap<String, Package>, version: &str) -> Result<String> {
    let mut out = String::from("## What's changed\n\n");
    out.push_str("| Crate | Version |\n| --- | --- |\n");
    for p in cx::releasable(packages) {
        out.push_str(&format!("| {} | {} |\n", p.name, p.version));
    }
    out.push('\n');
    for p in cx::releasable(packages) {
        let bullets = changelog::read_section(&p.dir.join("CHANGELOG.md"), version)?;
        if bullets.is_empty() {
            continue;
        }
        out.push_str(&format!("### {}\n\n", p.name));
        for b in bullets {
            out.push_str(&format!("- {b}\n"));
        }
        out.push('\n');
    }
    Ok(out)
}

pub fn run_update(args: crate::UpdateBodyArgs) -> Result<()> {
    let root = workspace_root()?;
    let version = args.tag.trim_start_matches('v');
    let packages = cx::load_packages(&root)?;
    let body = render(&packages, version)?;

    let rel_dir = root.join(".release");
    std::fs::create_dir_all(&rel_dir)?;
    let body_file = rel_dir.join("body.md");
    std::fs::write(&body_file, &body)?;
    println!("wrote {}", body_file.display());

    if args.publish {
        publish(&root, &args.tag, &body_file)?;
    }
    Ok(())
}

fn publish(root: &Path, tag: &str, body_file: &Path) -> Result<()> {
    let out = std::process::Command::new("gh")
        .current_dir(root)
        .args(["release", "edit", tag, "--notes-file"])
        .arg(body_file)
        .output()
        .context("run `gh release edit`")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("`gh release edit` failed:\n{stderr}");
    }
    println!("published release body for {tag}");
    Ok(())
}
