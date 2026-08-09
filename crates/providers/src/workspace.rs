//! Workspace discovery: expands bun/npm/pnpm workspaces into package dirs.
//!
//! Upone detects at the project root and at every workspace package, so a
//! monorepo where `drizzle.config.ts` lives under `packages/db` is still
//! recognized. This module only finds the package dirs; providers detect
//! inside each one.

use std::path::{Path, PathBuf};

/// Returns the workspace package directories below `root` (excluding root
/// itself). Returns an empty vec when the project is not a workspace.
#[must_use]
pub fn package_dirs(root: &Path) -> Vec<PathBuf> {
    let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut dirs = Vec::new();
    if let Some(globs) = package_json_workspaces(root) {
        collect_globs(root, &canon_root, &globs, &mut dirs);
    }
    if let Some(globs) = pnpm_workspaces(root) {
        collect_globs(root, &canon_root, &globs, &mut dirs);
    }
    // Keep only real packages (won't claim container/source dirs).
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Turns a relative package path into an injective task-id namespace.
///
/// Components are joined with `_` and any `_` inside a component is doubled,
/// so `packages/db` and a package literally named `packages_db` cannot share
/// a namespace (`packages_db` vs `packages__db`).
#[must_use]
pub fn dir_slug(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .map(|comp| comp.replace('_', "__"))
        .collect::<Vec<_>>()
        .join("_")
}

/// Expands workspace globs into package dirs that live inside the canonical
/// root (guards against `..`, absolute components and symlink escapes), then
/// drops candidates matched by `!exclusion` patterns.
fn collect_globs(root: &Path, canon_root: &Path, globs: &[String], out: &mut Vec<PathBuf>) {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    for glob in globs {
        if let Some(pat) = glob.strip_prefix('!') {
            excludes.push(pat.to_string());
        } else {
            includes.push(glob.clone());
        }
    }

    for glob in includes {
        expand_glob(root, canon_root, &glob, out);
    }

    if !excludes.is_empty() {
        out.retain(|dir| {
            let rel = dir.strip_prefix(root).map_or_else(
                |_| dir.display().to_string(),
                |p| p.to_string_lossy().into_owned(),
            );
            !excludes.iter().any(|pat| matches_glob(pat, &rel))
        });
    }
}

fn expand_glob(root: &Path, canon_root: &Path, glob: &str, out: &mut Vec<PathBuf>) {
    if let Some(rest) = glob.strip_suffix("/**") {
        walk_dirs(&root.join(rest), out, canon_root);
    } else if glob == "**" {
        walk_dirs(root, out, canon_root);
    } else if let Some(base) = glob.strip_suffix("/*") {
        let base = root.join(base);
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                push_package(&entry.path(), canon_root, out);
            }
        }
    } else {
        let path = root.join(glob);
        push_package(&path, canon_root, out);
    }
}

/// Recursively collects directories below `base` (descendants only), the
/// directory itself included, skipping anything that escapes `canon_root`.
/// Only directories that are real packages (have a package.json) are pushed.
fn walk_dirs(base: &Path, out: &mut Vec<PathBuf>, canon_root: &Path) {
    if !base.is_dir() || !inside(base, canon_root) {
        return;
    }
    if base != canon_root {
        push_package(base, canon_root, out);
    }
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_dirs(&path, out, canon_root);
            }
        }
    }
}

/// Pushes `path` when it is a package directory: inside the canonical root
/// and holding a package.json.
fn push_package(path: &Path, canon_root: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() && inside(path, canon_root) && path.join("package.json").is_file() {
        out.push(path.to_path_buf());
    }
}

/// True when the canonical path of `path` is a descendant of `canon_root`
/// (or equals it). Handles `..`, absolute components and symlinks.
fn inside(path: &Path, canon_root: &Path) -> bool {
    std::fs::canonicalize(path).is_ok_and(|canon| canon.starts_with(canon_root))
}

/// Minimal glob matcher supporting `*` (any chars except `/`) and `**` (any
/// chars including `/`), used to apply `!exclusion` workspace patterns.
fn matches_glob(pattern: &str, path: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    matches_glob_inner(&p, &s)
}

fn matches_glob_inner(p: &[char], s: &[char]) -> bool {
    if p.is_empty() {
        return s.is_empty();
    }
    match p[0] {
        '*' if p.get(1) == Some(&'*') => {
            if matches_glob_inner(&p[2..], s) {
                return true;
            }
            for i in 1..=s.len() {
                if matches_glob_inner(&p[2..], &s[i..]) {
                    return true;
                }
            }
            false
        }
        '*' => {
            if matches_glob_inner(&p[1..], s) {
                return true;
            }
            for i in 1..=s.len() {
                // A single `*` must not cross a path separator.
                if s[i - 1] == '/' {
                    break;
                }
                if matches_glob_inner(&p[1..], &s[i..]) {
                    return true;
                }
            }
            false
        }
        c if s.first().is_some_and(|sc| *sc == c) => matches_glob_inner(&p[1..], &s[1..]),
        _ => false,
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
        serde_json::Value::Object(map) => {
            map.get("packages").and_then(|p| p.as_array()).map(|list| {
                list.iter()
                    .filter_map(|g| g.as_str().map(String::from))
                    .collect()
            })?
        }
        _ => Vec::new(),
    };
    if globs.is_empty() {
        None
    } else {
        Some(globs)
    }
}

/// Reads only the top-level `packages:` block from pnpm-workspace.yaml (a
/// `- <glob>` list). Other YAML lists (onlyBuiltDependencies, etc.) are left
/// out of package detection.
fn pnpm_workspaces(root: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(root.join("pnpm-workspace.yaml")).ok()?;
    let mut globs = Vec::new();
    let mut in_packages = false;
    for line in text.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let content = line.trim();
        if in_packages {
            if let Some(rest) = content.strip_prefix('-') {
                let glob = rest.trim().trim_matches(['\'', '"']);
                if !glob.is_empty() {
                    globs.push(glob.to_string());
                }
            } else if !content.is_empty() && !indented {
                in_packages = false;
            }
        } else if content == "packages:" {
            in_packages = true;
        }
    }
    if globs.is_empty() {
        None
    } else {
        Some(globs)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("upone-ws-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn glob_matcher() {
        assert!(matches_glob("packages/*", "packages/db"));
        assert!(!matches_glob("packages/*", "packages/db/src"));
        assert!(matches_glob("packages/**", "packages/db"));
        assert!(matches_glob("packages/**", "packages/db/src"));
        assert!(!matches_glob("apps/**", "packages/db"));
        assert!(matches_glob("packages/tmp", "packages/tmp"));
        assert!(!matches_glob("packages/tmp", "packages/other"));
    }

    #[test]
    fn pnpm_parses_only_top_level_packages() {
        let dir = temp_dir("pnpm");
        fs::write(
            dir.join("pnpm-workspace.yaml"),
            "packages:\n  - \"packages/*\"\n  - \"apps/*\"\nonlyBuiltDependencies:\n  - esbuild\npackages_nested:\n  - nope\n",
        )
        .unwrap();
        let globs = pnpm_workspaces(&dir).unwrap();
        assert_eq!(globs, ["packages/*", "apps/*"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_handles_recursive_and_exclusions() {
        let root = temp_dir("rec");
        for sub in ["packages/db", "packages/api", "apps/web"] {
            fs::create_dir_all(root.join(sub)).unwrap();
            fs::write(root.join(sub).join("package.json"), "{}").unwrap();
        }
        // A non-package subdir should be excluded from the result.
        fs::create_dir_all(root.join("packages/db/src")).unwrap();
        let canon_root = fs::canonicalize(&root).unwrap();
        let mut out = Vec::new();
        collect_globs(
            &root,
            &canon_root,
            &["packages/**".to_string(), "!packages/api".to_string()],
            &mut out,
        );
        let rels: Vec<String> = out
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().display().to_string())
            .collect();
        assert_eq!(rels, ["packages/db"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_paths_outside_root() {
        let root = temp_dir("outside");
        fs::create_dir_all(root.join("packages")).unwrap();
        fs::write(root.join("packages").join("package.json"), "{}").unwrap();
        let canon_root = fs::canonicalize(&root).unwrap();
        let mut out = Vec::new();
        // A glob escaping the workspace root (resolves to the parent dir).
        collect_globs(&root, &canon_root, &["..".into()], &mut out);
        assert!(out.is_empty());
        collect_globs(&root, &canon_root, &["packages".into()], &mut out);
        assert_eq!(out.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }
}
