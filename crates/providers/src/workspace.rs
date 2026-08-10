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
    walk_dirs_inner(base, out, canon_root, 0);
}

/// Bounds recursion so a symlink loop pointing back inside the root (e.g.
/// `packages/loop -> .`) cannot recurse forever. `inside()` only rejects
/// escapes *outward*, so a cycle inside the root needs an explicit depth cap.
const MAX_DEPTH: usize = 32;

fn walk_dirs_inner(base: &Path, out: &mut Vec<PathBuf>, canon_root: &Path, depth: usize) {
    if depth > MAX_DEPTH || !base.is_dir() || !inside(base, canon_root) {
        return;
    }
    if base != canon_root {
        push_package(base, canon_root, out);
    }
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_dirs_inner(&path, out, canon_root, depth + 1);
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

/// Every directory to run detection on: the project root followed by each
/// workspace package directory.
fn all_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    dirs.extend(package_dirs(root));
    dirs
}

/// Relative display path of `dir` under `root`, or `None` when it is the root
/// itself.
fn rel_of<'a>(root: &'a Path, dir: &'a Path) -> Option<&'a Path> {
    dir.strip_prefix(root)
        .ok()
        .filter(|rel| !rel.as_os_str().is_empty())
}

/// Merged result of detecting and planning a project, including every
/// workspace package.
pub struct WorkspacePlan {
    /// Detections across the root and every package, root first, with the
    /// package location surfaced in each reason.
    pub detections: upone_core::Detected,
    /// Per-package detections paired with the context that detected them.
    pub package_detections: Vec<(upone_core::Context, upone_core::Detection)>,
    /// Merged plan: per-package task ids namespaced by directory slug, root
    /// tasks (install, etc.) keeping their canonical ids.
    pub plan: upone_core::Plan,
}

impl WorkspacePlan {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.detections.is_empty()
    }
}

/// Shared detection over the project root and every workspace package
/// (monorepos), deduplicating same provider+signature within a single package.
///
/// Returns the per-directory detections paired with the context that detected
/// them (used by the planner and the readiness sweep) plus a merged view for
/// the UI preview.
fn detect_workspace_dirs(
    root: &Path,
    registry: &upone_core::Registry,
) -> (
    Vec<(upone_core::Context, Vec<upone_core::Detection>)>,
    upone_core::Detected,
) {
    use std::collections::HashSet;

    let mut merged = upone_core::Detected::default();
    let mut per_dir: Vec<(upone_core::Context, Vec<upone_core::Detection>)> = Vec::new();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();

    for dir in &all_dirs(root) {
        let rel = rel_of(root, dir);
        let rel_display = rel.map(|r| r.display().to_string());
        let dir_ctx = upone_core::Context { cwd: dir.clone() };
        let dir_detections = upone_core::detect::detect(dir, registry);

        for d in &dir_detections.found {
            // Distinct packages may report the same provider+signature (e.g.
            // two packages with drizzle); keep them separate. Within one
            // package a provider matches at most once, so no further dedup.
            let key = (
                rel_display.clone().unwrap_or_default(),
                d.provider.to_string(),
                d.signature.clone(),
            );
            if !seen.insert(key) {
                continue;
            }
            let reason = rel_display
                .as_ref()
                .map_or_else(|| d.reason.clone(), |r| format!("{0} ({r})", d.reason));
            merged.found.push(upone_core::Detection {
                provider: d.provider,
                signature: d.signature.clone(),
                reason,
            });
        }

        per_dir.push((dir_ctx, dir_detections.found));
    }

    (per_dir, merged)
}

/// Detects the project at the root and at every workspace package
/// (monorepos), deduplicating same provider+signature within a package.
///
/// Returns the detections for the UI preview and the per-package list used by
/// the readiness sweep.
#[must_use]
pub fn detect_workspace(
    ctx: &upone_core::Context,
    registry: &upone_core::Registry,
) -> (
    upone_core::Detected,
    Vec<(upone_core::Context, upone_core::Detection)>,
) {
    let (per_dir, detections) = detect_workspace_dirs(&ctx.cwd, registry);
    let package_detections = per_dir
        .into_iter()
        .flat_map(|(dir_ctx, found)| found.into_iter().map(move |d| (dir_ctx.clone(), d)))
        .collect();
    (detections, package_detections)
}

/// Plans every provider with its own working directory, then merges each
/// per-package plan into one whose task ids are namespaced by slug.
///
/// Tasks may depend on tasks living outside their package (e.g. the root
/// `bun-install`); those edges are validated once the plans merge.
///
/// # Errors
///
/// Returns an error when a per-package or the merged plan fails to build
/// (duplicate ids or dependency cycles).
pub fn plan_workspace(
    ctx: &upone_core::Context,
    registry: &upone_core::Registry,
) -> Result<WorkspacePlan, String> {
    use std::collections::HashSet;
    use upone_core::{Planner, Task};

    let root = ctx.cwd.clone();
    let (per_dir, detections) = detect_workspace_dirs(&root, registry);
    let mut pkg_detections: Vec<(upone_core::Context, upone_core::Detection)> = Vec::new();
    let mut planner = Planner::new(ctx);

    for (dir_ctx, dir_detections) in per_dir {
        let rel = rel_of(&root, &dir_ctx.cwd);
        let slug = rel.map(dir_slug);

        // Plan this directory's providers with its own cwd so tasks built
        // here know where to run (e.g. `drizzle-kit generate` in packages/db).
        let mut sub_planner = Planner::new(&dir_ctx);
        for d in &dir_detections {
            if let Some(provider) = registry.all().iter().find(|p| p.id() == d.provider) {
                provider.plan(&dir_ctx, &mut sub_planner);
            }
        }
        // Relaxed: a package may depend on a task outside this planner
        // (e.g. the root install), validated once all plans merge below.
        let local_plan = sub_planner
            .build_allow_external()
            .map_err(|e| format!("failed to build the plan: {e}"))?;

        pkg_detections.extend(dir_detections.iter().cloned().map(|d| (dir_ctx.clone(), d)));

        // Namespace per-package task ids so the same tech in two packages
        // doesn't collide. Root tasks (install etc.) keep their canonical ids.
        let local_ids: HashSet<String> = local_plan.ids().into_iter().collect();
        for id in local_plan.ids() {
            let Some(task) = local_plan.task(&id).cloned() else {
                continue;
            };
            let (new_id, new_deps) = match &slug {
                None => (id, task.deps),
                Some(s) => (
                    format!("{s}-{id}"),
                    task.deps
                        .into_iter()
                        .map(|d| {
                            if local_ids.contains(&d) {
                                format!("{s}-{d}")
                            } else {
                                d
                            }
                        })
                        .collect(),
                ),
            };
            planner.add(Task {
                id: new_id,
                deps: new_deps,
                ..task
            });
        }
    }

    let plan = planner
        .build()
        .map_err(|e| format!("failed to build the plan: {e}"))?;

    Ok(WorkspacePlan {
        detections,
        package_detections: pkg_detections,
        plan,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        crate::testkit::temp_dir("ws", name)
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

    #[cfg(unix)]
    #[test]
    fn walk_dirs_terminates_on_symlink_loop() {
        use std::os::unix::fs::symlink;

        let root = temp_dir("loop");
        fs::create_dir_all(root.join("packages")).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("packages").join("package.json"), "{}").unwrap();
        // A cycle inside the root: `packages/back` resolves to the root,
        // which `inside()` accepts because it does not escape outward.
        symlink(&root, root.join("packages").join("back")).unwrap();

        let canon_root = fs::canonicalize(&root).unwrap();
        let mut out = Vec::new();
        walk_dirs(&root, &mut out, &canon_root);
        let _ = fs::remove_dir_all(&root);

        // Must not recurse forever; only reachable dirs are reported.
        let rels: Vec<String> = out
            .iter()
            .map(|p| p.strip_prefix(&canon_root).unwrap().display().to_string())
            .collect();
        assert!(rels.contains(&String::from("packages")));
        assert_eq!(rels.len(), out.len());
    }

    fn write_ws_package(root: &Path, rel: &str, files: &[(&str, &str)]) {
        let dir = root.join(rel);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("package.json"),
            serde_json::json!({ "name": rel }).to_string(),
        )
        .unwrap();
        for (name, content) in files {
            fs::write(dir.join(name), content).unwrap();
        }
    }

    #[test]
    fn monorepo_plan_namespaces_package_tasks() {
        let root = temp_dir("mono");
        fs::write(
            root.join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        fs::write(root.join("bun.lock"), "").unwrap();
        write_ws_package(
            &root,
            "packages/db",
            &[("drizzle.config.ts", "export default {};")],
        );

        let ctx = upone_core::Context { cwd: root.clone() };
        let ws = plan_workspace(&ctx, &crate::build_registry()).unwrap();

        let mut ids = ws.plan.ids();
        ids.sort();
        assert!(ids.contains(&"bun-install".into()));
        assert!(ids.contains(&"packages_db-drizzle-check".into()));
        assert!(ids.contains(&"packages_db-drizzle-generate".into()));
        // Root tasks keep their canonical ids (no slug prefix).
        assert!(!ids.iter().any(|i| i.starts_with("bun_")));

        let install = ws.plan.task(&"bun-install".into()).unwrap();
        assert_eq!(install.deps, ["bun-check"]);

        // The package's generate depends on its own check and the root install.
        let gen = ws
            .plan
            .task(&"packages_db-drizzle-generate".into())
            .unwrap();
        let mut deps = gen.deps.clone();
        deps.sort();
        assert_eq!(deps, ["bun-install", "packages_db-drizzle-check"]);

        let drizzle = ws
            .detections
            .found
            .iter()
            .find(|d| d.provider == "drizzle")
            .unwrap();
        assert!(drizzle.reason.contains("(packages/db)"));
        assert!(!ws.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn monorepo_two_packages_same_tech_stay_distinct() {
        let root = temp_dir("mono2");
        fs::write(
            root.join("package.json"),
            r#"{"name":"root","workspaces":["packages/*","apps/*"]}"#,
        )
        .unwrap();
        fs::write(root.join("bun.lock"), "").unwrap();
        write_ws_package(
            &root,
            "packages/db",
            &[("drizzle.config.ts", "export default {};")],
        );
        write_ws_package(
            &root,
            "apps/web",
            &[("drizzle.config.ts", "export default {};")],
        );

        let ctx = upone_core::Context { cwd: root.clone() };
        let ws = plan_workspace(&ctx, &crate::build_registry()).unwrap();

        let ids = ws.plan.ids();
        assert!(ids.contains(&"packages_db-drizzle-check".into()));
        assert!(ids.contains(&"apps_web-drizzle-check".into()));
        assert_eq!(
            ids.iter().filter(|i| i.ends_with("-drizzle-check")).count(),
            2
        );

        let db_gen = ws
            .plan
            .task(&"packages_db-drizzle-generate".into())
            .unwrap();
        assert!(db_gen.deps.contains(&"packages_db-drizzle-check".into()));
        assert!(!db_gen.deps.contains(&"apps_web-drizzle-check".into()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn detect_workspace_reports_package_location_and_dedups() {
        let root = temp_dir("detect");
        fs::write(
            root.join("package.json"),
            r#"{"name":"root","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        fs::write(root.join("bun.lock"), "").unwrap();
        write_ws_package(
            &root,
            "packages/db",
            &[("drizzle.config.ts", "export default {};")],
        );

        let ctx = upone_core::Context { cwd: root.clone() };
        let (detected, pkg_dets) = detect_workspace(&ctx, &crate::build_registry());

        // The root bun detection plus the package drizzle detection.
        assert!(detected.found.iter().any(|d| d.provider == "bun"));
        let drizzle = detected
            .found
            .iter()
            .filter(|d| d.provider == "drizzle")
            .collect::<Vec<_>>();
        assert_eq!(drizzle.len(), 1);
        assert!(drizzle[0].reason.contains("(packages/db)"));
        assert_eq!(pkg_dets.len(), 2);
        assert_eq!(detected.found.len(), 2);

        let _ = fs::remove_dir_all(&root);
    }
}
