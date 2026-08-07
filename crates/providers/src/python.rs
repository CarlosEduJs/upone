//! Shared helpers for the Python package-manager providers (uv / poetry / pip).

use std::path::{Path, PathBuf};

use upone_core::plan::RunOutcome;
use upone_core::run::RunError;

use crate::cmd::which;

/// Cross-platform path to the `python` binary inside a project venv.
#[must_use]
pub fn venv_python(cwd: &Path) -> PathBuf {
    if cfg!(windows) {
        cwd.join(VENV_DIR).join("Scripts").join("python.exe")
    } else {
        cwd.join(VENV_DIR).join("bin").join("python")
    }
}

/// True when the project venv already exists (has a working interpreter).
#[must_use]
pub fn venv_exists(cwd: &Path) -> bool {
    venv_python(cwd).is_file()
}

const VENV_DIR: &str = ".venv";

/// Name of a Python interpreter on PATH, preferring `python3`.
#[must_use]
pub fn python_bin() -> Option<&'static str> {
    if which("python3") {
        Some("python3")
    } else if which("python") {
        Some("python")
    } else {
        None
    }
}

/// True when a requirements manifest exists in `cwd`.
#[must_use]
pub fn has_requirements(cwd: &Path) -> bool {
    requirements_file(cwd).is_some()
}

/// Finds the canonical requirements file to install.
///
/// Preference: `requirements.txt`, a sibling `requirements*.txt`, otherwise
/// any `*.txt` inside a `requirements/` directory. Returns the
/// lexicographically first candidate when there are several.
#[must_use]
pub fn requirements_file(cwd: &Path) -> Option<PathBuf> {
    let direct = cwd.join("requirements.txt");
    if direct.is_file() {
        return Some(direct);
    }

    let mut candidates = Vec::new();
    if let Ok(entries) = std::fs::read_dir(cwd) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_file() && name.starts_with("requirements") && name.ends_with(".txt") {
                candidates.push(path);
            }
        }
    }

    let req_dir = cwd.join("requirements");
    if let Ok(entries) = std::fs::read_dir(&req_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "txt") {
                candidates.push(path);
            }
        }
    }

    candidates.sort();
    candidates.into_iter().next()
}

/// Shared binary-on-PATH check with an install hint on failure.
///
/// # Errors
///
/// Fails with a `RunError::Failed` when `bin` is not on PATH.
pub fn check_binary(
    bin: &str,
    hint: &str,
    emit: &mut dyn FnMut(&str),
) -> Result<RunOutcome, RunError> {
    if which(bin) {
        emit(&format!("{bin} found on PATH"));
        Ok(RunOutcome::Ran(format!("{bin} available")))
    } else {
        Err(RunError::Failed(format!("{bin} not found on PATH. {hint}")))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("upone-py-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn requirements_prefers_direct_then_siblings_then_dir() {
        let dir = temp_dir("req");
        assert!(!has_requirements(&dir));

        fs::write(dir.join("requirements-dev.txt"), "requests\n").unwrap();
        let found = requirements_file(&dir).unwrap();
        assert_eq!(
            found.file_name().unwrap().to_string_lossy(),
            "requirements-dev.txt"
        );
        assert!(has_requirements(&dir));

        fs::write(dir.join("requirements.txt"), "flask\n").unwrap();
        let found = requirements_file(&dir).unwrap();
        assert_eq!(
            found.file_name().unwrap().to_string_lossy(),
            "requirements.txt"
        );

        let sub = dir.join("requirements");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("base.txt"), "click\n").unwrap();
        let found = requirements_file(&dir).unwrap();
        assert_eq!(
            found.file_name().unwrap().to_string_lossy(),
            "requirements.txt"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn requirements_dir_is_a_fallback() {
        let dir = temp_dir("reqdir");
        let sub = dir.join("requirements");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("base.txt"), "click\n").unwrap();
        let found = requirements_file(&dir).unwrap();
        assert_eq!(found.file_name().unwrap().to_string_lossy(), "base.txt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn venv_python_probes_bin_dir() {
        let dir = temp_dir("venv");
        assert!(!venv_exists(&dir));
        fs::create_dir_all(dir.join(".venv/bin")).unwrap();
        fs::write(dir.join(".venv/bin/python"), "#!/bin/sh\n").unwrap();
        assert!(venv_exists(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
