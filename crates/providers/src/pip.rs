//! `pip` provider: detects requirements manifests (in the absence of uv.lock
//! and poetry.lock) and installs them into a project-local venv.
//!
//! Deps are never installed into the system interpreter: upone creates a
//! `.venv` first and runs pip through it. The venv task is idempotent (skips
//! when the venv already exists).

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::ReadinessCheck;
use upone_core::run::RunError;
use upone_core::{Context, Detection, Risk};

use crate::cmd::spawn_cmd;

use super::python;

pub struct Pip;

impl Provider for Pip {
    fn id(&self) -> &'static str {
        "pip"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["requirements.txt"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        // Lowest priority Python manager: only when no stronger lockfile is
        // present, so a poetry/uv project never also triggers a pip install.
        if python::has_requirements(cwd)
            && !cwd.join("uv.lock").is_file()
            && !cwd.join("poetry.lock").is_file()
        {
            return Some(self.found("requirements.txt"));
        }
        None
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "pip-check",
            "check python installed",
            "checks that a python interpreter is on PATH",
        )
        .risk(Risk::Low)
        .run(pip_check);

        let venv = Task::new(
            "pip-venv",
            "create project venv",
            "creates .venv with 'python -m venv' if it does not exist",
        )
        .risk(Risk::Medium)
        .depends_on(["pip-check"])
        .run(pip_venv);

        let install = Task::new(
            "pip-install",
            "pip install",
            "installs the requirements file into the project venv (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["pip-venv"])
        .run(pip_install);

        planner.add(check);
        planner.add(venv);
        planner.add(install);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        vec![python::venv_check(
            "pip-venv",
            ".venv exists and holds a python interpreter",
            "Run 'pip-venv' via 'upone up' or 'python3 -m venv .venv'",
            &ctx.cwd,
        )]
    }
}

fn pip_check(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    python::python_bin().map_or_else(
        || {
            Err(RunError::Failed(
                "python not found on PATH. Install it via https://www.python.org/downloads/".into(),
            ))
        },
        |bin| {
            emit(&format!("{bin} found on PATH"));
            Ok(RunOutcome::Ran(format!("{bin} available")))
        },
    )
}

fn pip_venv(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if python::venv_exists(&ctx.cwd) {
        emit(".venv already present, skipping");
        return Ok(RunOutcome::Skipped(".venv present".into()));
    }
    let Some(bin) = python::python_bin() else {
        return Err(RunError::Failed(
            "python not found on PATH, cannot create the venv".into(),
        ));
    };
    spawn_cmd(bin, &["-m", "venv", ".venv"], &ctx.cwd, emit)
}

fn pip_install(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let Some(req) = python::requirements_file(&ctx.cwd) else {
        return Err(RunError::Failed(
            "no requirements file found to install".into(),
        ));
    };
    let venv = python::venv_python(&ctx.cwd);
    let venv_str = venv.to_string_lossy().into_owned();
    let req_str = req.to_string_lossy().into_owned();
    let args = ["-m", "pip", "install", "-r", req_str.as_str()];
    spawn_cmd(&venv_str, &args, &ctx.cwd, emit)
}
