//! `version` — aggregate `.changes/` notes, bump versions and write changelogs.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use semver::Version;

use crate::changes::{self, Bump, Note};
use crate::{changelog, cx, release_body, workspace_root};

/// Packages whose version can be bumped by this tool.
const RELEASABLE: [&str; 3] = ["upone", "upone-core", "upone-providers"];

pub fn run(args: crate::VersionArgs) -> Result<()> {
    let root = workspace_root()?;
    let mut packages = cx::load_packages(&root)?;
    // All releasable crates must exist before any indexing below.
    for name in RELEASABLE {
        anyhow::ensure!(
            packages.contains_key(name),
            "release package not found in workspace: {name}"
        );
    }
    let old_versions: BTreeMap<String, Version> = packages
        .iter()
        .map(|(k, p)| (k.clone(), p.version.clone()))
        .collect();
    let notes = changes::read_notes(&root)?;

    if notes.is_empty() {
        println!("no active changesets under .changes/; nothing to do");
        return Ok(());
    }

    // Group notes by package and keep the highest bump per package.
    let mut bumps: BTreeMap<String, Bump> = BTreeMap::new();
    let mut note_groups: BTreeMap<String, Vec<&Note>> = BTreeMap::new();
    for note in &notes {
        let pkg = cx::known_package(&packages, &note.package)?;
        let e = bumps.entry(pkg.clone()).or_insert(Bump::Patch);
        if note.bump > *e {
            *e = note.bump;
        }
        note_groups.entry(pkg.clone()).or_default().push(note);
    }

    // The shipped binary must also bump when any of its library deps bump.
    let dep_bump = ["upone-core", "upone-providers"]
        .iter()
        .filter_map(|p| bumps.get(*p))
        .copied()
        .max();
    if let Some(b) = dep_bump {
        let e = bumps.entry("upone".to_string()).or_insert(Bump::Patch);
        if b > *e {
            *e = b;
        }
    }

    // Compute new versions for bumped packages only.
    let mut new_versions: BTreeMap<String, Version> = BTreeMap::new();
    for name in RELEASABLE.iter() {
        if let Some(b) = bumps.get(*name) {
            let current = &packages[*name].version;
            let next = bumped(current, *b);
            new_versions.insert(name.to_string(), next);
        }
    }

    let release_version = new_versions
        .get("upone")
        .cloned()
        .or_else(|| packages.get("upone").map(|p| p.version.clone()))
        .context("upone package missing")?;

    if args.dry_run {
        println!("would release v{release_version}");
        for name in RELEASABLE.iter() {
            let msg = match new_versions.get(*name) {
                Some(next) => format!("  {name}: {} -> {next}", old_versions[*name]),
                None => format!("  {name}: {} (unchanged)", old_versions[*name]),
            };
            println!("{msg}");
        }
        for (name, group) in &note_groups {
            println!("  notes for {name}:");
            for n in group {
                println!("    - {}", first_line(&n.summary));
            }
        }
        return Ok(());
    }

    // 1. Write new versions into each crate manifest.
    for (name, next) in &new_versions {
        let dir = &packages[name].dir;
        set_crate_version(&dir.join("Cargo.toml"), next)?;
    }

    // 2. Run cargo check to update Cargo.lock with the new versions.
    let status = std::process::Command::new("cargo")
        .current_dir(&root)
        .arg("check")
        .status()
        .context("run cargo check to update Cargo.lock")?;
    anyhow::ensure!(status.success(), "cargo check failed after version bump");

    // 3. Re-resolve so Cargo.lock and package versions reflect the bump.
    packages = cx::load_packages(&root)?;

    // 3. Per-crate changelogs.
    let mut bullets_by_crate: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, group) in &note_groups {
        let bullets: Vec<String> = group
            .iter()
            .flat_map(|n| n.summary.lines())
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        bullets_by_crate.insert(name.clone(), bullets);
    }
    // Add dependency-update bullets to upone when a lib bumped.
    let bumped_deps: Vec<String> = ["upone-core", "upone-providers"]
        .iter()
        .filter(|p| new_versions.contains_key(**p))
        .map(|p| format!("update {p} {} -> {}", old_versions[*p], new_versions[*p]))
        .collect();
    if !bumped_deps.is_empty() {
        bullets_by_crate
            .entry("upone".to_string())
            .or_default()
            .extend(bumped_deps);
    }

    for name in RELEASABLE.iter() {
        if let Some(next) = new_versions.get(*name) {
            let bullets = bullets_by_crate.get(*name).cloned().unwrap_or_default();
            changelog::prepend_section(
                &packages[*name].dir.join("CHANGELOG.md"),
                &next.to_string(),
                &bullets,
            )?;
        }
    }

    // 4. Root changelog with a crate-version table + per-crate sections.
    let root_changelog = root.join("CHANGELOG.md");
    let mut block = format!("## {release_version}\n\n");
    block.push_str("Crate versions in this release:\n\n");
    block.push_str("| Crate | Version |\n| --- | --- |\n");
    for name in RELEASABLE.iter() {
        block.push_str(&format!("| {name} | {} |\n", packages[*name].version));
    }
    block.push('\n');
    for name in RELEASABLE.iter() {
        if let Some(bullets) = bullets_by_crate.get(*name) {
            if bullets.is_empty() {
                continue;
            }
            block.push_str(&format!("### {name}\n\n"));
            for b in bullets {
                block.push_str(&format!("- {b}\n"));
            }
            block.push('\n');
        }
    }
    changelog::prepend_raw(&root_changelog, &release_version.to_string(), &block)?;

    // 5. Archive consumed notes.
    changes::archive_notes(&root, &release_version.to_string(), &notes)?;

    // 6. Emit artifacts used by CI.
    let rel_dir = root.join(".release");
    std::fs::create_dir_all(&rel_dir)?;
    std::fs::write(rel_dir.join("version"), release_version.to_string())?;
    let body = release_body::render(&packages, &release_version.to_string())?;
    std::fs::write(rel_dir.join("body.md"), &body)?;

    println!("release v{release_version} prepared (see .release/body.md)");
    for name in RELEASABLE.iter() {
        let msg = match new_versions.get(*name) {
            Some(next) => format!("  {name}: {} -> {next}", old_versions[*name]),
            None => format!("  {name}: {} (unchanged)", old_versions[*name]),
        };
        println!("{msg}");
    }
    Ok(())
}

fn bumped(v: &Version, b: Bump) -> Version {
    match b {
        Bump::Patch => Version {
            major: v.major,
            minor: v.minor,
            patch: v.patch + 1,
            pre: Default::default(),
            build: Default::default(),
        },
        Bump::Minor => Version {
            major: v.major,
            minor: v.minor + 1,
            patch: 0,
            pre: Default::default(),
            build: Default::default(),
        },
        Bump::Major => Version {
            major: v.major + 1,
            minor: 0,
            patch: 0,
            pre: Default::default(),
            build: Default::default(),
        },
    }
}

/// Replaces the `version` line inside the `[package]` table of a manifest.
fn set_crate_version(manifest: &Path, version: &Version) -> Result<()> {
    let content = std::fs::read_to_string(manifest)
        .with_context(|| format!("read {}", manifest.display()))?;
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let mut in_package = false;
    let mut replaced = false;
    for line in lines.iter_mut() {
        let t = line.trim();
        if t == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && t.starts_with('[') {
            break;
        }
        if in_package && t.starts_with("version") {
            *line = format!("version = \"{version}\"");
            replaced = true;
            break;
        }
    }
    if !replaced {
        anyhow::bail!("no version line found in {}", manifest.display());
    }
    std::fs::write(manifest, lines.join("\n") + "\n")?;
    Ok(())
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}
