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

/// Resolves the JS install task id detected in the project (if any).
/// Used by providers that depend on node_modules (prisma, drizzle).
pub fn js_install_task(cwd: &Path) -> Option<&'static str> {
    let markers: [(&str, &str); 4] = [
        ("bun.lock", "bun-install"),
        ("bun.lockb", "bun-install"),
        ("pnpm-lock.yaml", "pnpm-install"),
        ("package-lock.json", "npm-install"),
    ];
    markers
        .iter()
        .find(|(f, _)| cwd.join(f).is_file())
        .map(|(_, id)| *id)
}
