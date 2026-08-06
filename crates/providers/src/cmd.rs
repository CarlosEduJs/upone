//! Shared helpers to run commands safely and explainably.

use std::path::Path;
use std::process::Command;

use upone_core::plan::RunOutcome;
use upone_core::run::RunError;

/// Runs a command with the given args in `cwd`, streaming output to `emit`.
pub fn spawn_cmd(
    program: &str,
    args: &[&str],
    cwd: &Path,
    emit: &mut dyn FnMut(&str),
) -> Result<RunOutcome, RunError> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;

    if let Ok(stderr) = String::from_utf8(output.stderr.clone()) {
        if !stderr.trim().is_empty() {
            emit(&stderr);
        }
    }
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    if !stdout.trim().is_empty() {
        emit(&stdout);
    }

    if output.status.success() {
        let summary = stdout.lines().last().unwrap_or("ok").trim().to_string();
        Ok(RunOutcome::Ran(summary))
    } else {
        // Some tools (e.g. pnpm) report errors on stdout, so fall back to
        // it when stderr is empty. Use the tail of the output: the real
        // failure message usually shows up last (compose pull/start lines
        // precede the actual error).
        let stderr = String::from_utf8(output.stderr).unwrap_or_default();
        let lines: Vec<String> = stderr
            .lines()
            .chain(stdout.lines())
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        let start = lines.len().saturating_sub(3);
        let detail = lines[start..].join(" | ");
        Err(RunError::Failed(format!(
            "`{} {}` failed: {}",
            program,
            args.join(" "),
            if detail.is_empty() {
                "no output".to_string()
            } else {
                detail
            }
        )))
    }
}

/// Checks if a program exists on PATH.
pub fn which(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Returns true if `needle` appears (case-insensitive) anywhere in the given files.
pub fn files_contain(cwd: &Path, files: &[&str], needles: &[&str]) -> bool {
    files.iter().any(|file| {
        let path = cwd.join(file);
        let ok = std::fs::read_to_string(&path);
        match ok {
            Ok(content) => {
                let lower = content.to_lowercase();
                needles.iter().any(|n| lower.contains(&n.to_lowercase()))
            }
            Err(_) => false,
        }
    })
}

/// Returns true if any of the files exists in `cwd`.
pub fn any_exists(cwd: &Path, files: &[&str]) -> bool {
    files.iter().any(|f| cwd.join(f).exists())
}

/// Resolves the host port a service is published on in the given compose files.
///
/// Matches port mappings like `- "15432:5432"` / `- 15432:5432` where the
/// *container* side equals `container_port` and returns the *host* side, so
/// providers can verify the real published port instead of assuming 5432/6379.
/// Falls back to `container_port` when nothing is configured or parseable.
pub fn compose_host_port(cwd: &Path, files: &[&str], container_port: u16) -> u16 {
    let needle = format!(":{container_port}");
    for file in files {
        let Ok(content) = std::fs::read_to_string(cwd.join(file)) else {
            continue;
        };
        for line in content.lines() {
            let Some(pos) = line.find(&needle) else {
                continue;
            };
            let mut host = line[..pos].trim();
            host = host.trim_matches(['"', '\'', '-', ' ', ':', '[', ']', '+']);
            if let Ok(port) = host.parse::<u16>() {
                return port;
            }
        }
    }
    container_port
}

/// Returns true if `dependency` is a key in the package.json dependencies,
/// devDependencies, peerDependencies or optionalDependencies maps of `cwd`.
/// Used for dependency-based detections (trpc, better-auth, next) that have no
/// dedicated config file. Parses the manifest as JSON so unrelated keys (e.g.
/// `scripts.next` or `overrides.next`) can never trigger a match.
pub fn package_has_dependency(cwd: &Path, dependency: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(cwd.join("package.json")) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .any(|section| {
        value
            .get(*section)
            .and_then(|map| map.as_object())
            .is_some_and(|map| map.contains_key(dependency))
    })
}

/// Returns true when a `node_modules` directory exists at or above `cwd`.
///
/// Workspace roots hoist dependencies, so a package may not hold its own
/// node_modules (npm/pnpm/yarn hoist to the root, bun links them there too).
/// Mirrors the upward-resolution behavior of [`js_install_task`].
pub fn node_modules_present(cwd: &Path) -> bool {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if d.join("node_modules").is_dir() {
            return true;
        }
        dir = d.parent();
    }
    false
}

/// Resolves the JS install task id detected in the project (if any).
/// Used by providers that depend on node_modules (prisma, drizzle).
///
/// In a monorepo the lockfile usually lives at the workspace root while the
/// package sits deeper, so this walks up from `cwd` to the filesystem root.
pub fn js_install_task(cwd: &Path) -> Option<&'static str> {
    let markers: [(&str, &str); 4] = [
        ("bun.lock", "bun-install"),
        ("bun.lockb", "bun-install"),
        ("pnpm-lock.yaml", "pnpm-install"),
        ("package-lock.json", "npm-install"),
    ];
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if let Some((_, id)) = markers.iter().find(|(f, _)| d.join(f).is_file()) {
            return Some(*id);
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("upone-cmd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn package_has_dependency_checks_dep_maps_only() {
        let dir = temp_dir("dep");
        fs::write(
            dir.join("package.json"),
            r#"{
  "name": "pkg",
  "scripts": { "next": "echo nope" },
  "overrides": { "better-auth": "1.0.0" },
  "dependencies": { "next": "15.0.0" },
  "devDependencies": { "@trpc/server": "^11.0.0" },
  "peerDependencies": { "zod": "4.0.0" },
  "optionalDependencies": { "esbuild": "0.25.0" }
}"#,
        )
        .unwrap();
        assert!(package_has_dependency(&dir, "next"));
        assert!(package_has_dependency(&dir, "@trpc/server"));
        assert!(package_has_dependency(&dir, "zod"));
        assert!(package_has_dependency(&dir, "esbuild"));
        // Present only in scripts/overrides must not match.
        assert!(!package_has_dependency(&dir, "better-auth"));
        assert!(!package_has_dependency(&dir, "echo"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_has_dependency_missing_or_broken_manifest() {
        let dir = temp_dir("bad");
        assert!(!package_has_dependency(&dir, "next"));
        fs::write(dir.join("package.json"), "{not json").unwrap();
        assert!(!package_has_dependency(&dir, "next"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_modules_present_walks_up() {
        let dir = temp_dir("nm").join("a").join("b");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(temp_dir("nm").join("node_modules")).unwrap();
        assert!(node_modules_present(&dir));
        let _ = fs::remove_dir_all(temp_dir("nm"));
    }
}
