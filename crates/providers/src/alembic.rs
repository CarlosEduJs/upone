//! `alembic` provider: detects `alembic.ini` and applies migrations into the
//! project venv with `python -m alembic upgrade head`.
//!
//! Alembic runs inside the project's `.venv` (the `pip`/`uv`/`poetry`
//! providers create it), so it only runs when the venv is already prepared.
//! Both `alembic` and `sqlite` may report for the same project: alembic owns
//! the migrations while sqlite ensures the database file exists.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::ReadinessCheck;
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{add_migration_plan, spawn_cmd, DbWiring, InstallKind};

pub struct Alembic;

impl Provider for Alembic {
    fn id(&self) -> &'static str {
        "alembic"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["alembic.ini"]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if cwd.join("alembic.ini").is_file() {
            return Some(self.found("alembic.ini"));
        }
        None
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "alembic-check",
            "check python venv for alembic",
            "checks that the project venv has the alembic module installed",
        )
        .risk(Risk::Low)
        .run(check_alembic);

        let upgrade = Task::new(
            "alembic-upgrade",
            "alembic upgrade head",
            "applies pending alembic migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .run(alembic_upgrade);

        add_migration_plan(
            planner,
            ctx,
            check,
            upgrade,
            InstallKind::Python,
            DbWiring::Database,
        );
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        vec![super::python::venv_check(
            "alembic-venv",
            ".venv exists so alembic can run",
            "Run your python package manager's install (or 'upone up') to create it",
            &ctx.cwd,
        )]
    }
}

fn check_alembic(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !super::python::venv_exists(&ctx.cwd) {
        return Err(RunError::Failed(
            "project venv missing. Install the python dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    emit("venv present");
    let venv = super::python::venv_python(&ctx.cwd);
    let venv_str = venv.to_string_lossy().into_owned();
    spawn_cmd(&venv_str, &["-m", "alembic", "--version"], &ctx.cwd, emit)
}

fn alembic_upgrade(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !super::python::venv_exists(&ctx.cwd) {
        return Err(RunError::Failed(
            "project venv missing, cannot run alembic. Install `alembic` into the venv first."
                .into(),
        ));
    }
    let venv = super::python::venv_python(&ctx.cwd);
    let venv_str = venv.to_string_lossy().into_owned();
    spawn_cmd(
        &venv_str,
        &["-m", "alembic", "upgrade", "head"],
        &ctx.cwd,
        emit,
    )
}
