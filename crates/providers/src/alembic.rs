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
use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{migration_db_dep, python_install_task, spawn_cmd};

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
        let mut check = Task::new(
            "alembic-check",
            "check python venv for alembic",
            "checks that the project venv has the alembic module installed",
        )
        .risk(Risk::Low)
        .run(check_alembic);

        let mut upgrade = Task::new(
            "alembic-upgrade",
            "alembic upgrade head",
            "applies pending alembic migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .depends_on(["alembic-check"])
        .run(alembic_upgrade);

        if let Some(install) = python_install_task(&ctx.cwd) {
            check = check.depends_on([install]);
            upgrade = upgrade.depends_on(["alembic-check", install]);
        }
        if let Some(db) = migration_db_dep(&ctx.cwd) {
            upgrade = upgrade.depends_on(["alembic-check", db]);
        }

        planner.add(check);
        planner.add(upgrade);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        let cwd = ctx.cwd.clone();
        vec![ReadinessCheck::new(
            "alembic-venv",
            "project venv (.venv)",
            ".venv exists so alembic can run",
            Importance::Required,
            move |_ctx| {
                if super::python::venv_exists(&cwd) {
                    ReadinessStatus::Ready(".venv present".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: ".venv not found".into(),
                        remedy:
                            "Run your python package manager's install (or 'upone up') to create it"
                                .into(),
                    }
                }
            },
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
