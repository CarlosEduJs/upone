//! `knex` provider: detects the Knex query builder via the `knex` dependency
//! or a `knexfile.*` and runs `knex migrate:latest`.
//!
//! Migrations apply the schema to the configured database, so the task only
//! runs after deps are installed and, when reported, after the DB is up.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{js_install_task, migration_db_dep, package_has_dependency, spawn_cmd, which};

const KNEXFILES: &[&str] = &["knexfile.ts", "knexfile.js", "knexfile.mts", "knexfile.cts"];

pub struct Knex;

impl Provider for Knex {
    fn id(&self) -> &'static str {
        "knex"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if package_has_dependency(cwd, "knex") {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "package.json (knex)".into(),
                reason: "Knex query builder detected".into(),
            });
        }
        for file in KNEXFILES {
            if cwd.join(file).is_file() {
                return Some(upone_core::Detection {
                    provider: self.id(),
                    signature: file.to_string(),
                    reason: format!("found {file}"),
                });
            }
        }
        None
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        let mut check = Task::new(
            "knex-check",
            "check knex available",
            "checks dependencies and the knex CLI",
        )
        .risk(Risk::Low)
        .run(check_knex);

        let mut migrate = Task::new(
            "knex-migrate",
            "knex migrate:latest",
            "applies pending knex migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .depends_on(["knex-check"])
        .run(knex_migrate);

        if let Some(install) = js_install_task(&ctx.cwd) {
            check = check.depends_on([install]);
            migrate = migrate.depends_on(["knex-check", install]);
        }
        if let Some(db) = migration_db_dep(&ctx.cwd) {
            migrate = migrate.depends_on(["knex-check", db]);
        }

        planner.add(check);
        planner.add(migrate);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        let cwd = ctx.cwd.clone();
        vec![ReadinessCheck::new(
            "knex-deps",
            "knex dependencies installed",
            "node_modules present for knex",
            Importance::Required,
            move |_ctx| {
                if crate::cmd::node_modules_present(&cwd) {
                    ReadinessStatus::Ready("node_modules present".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "node_modules missing for knex".into(),
                        remedy: "Run your package manager's install or 'upone up'".into(),
                    }
                }
            },
        )]
    }
}

fn check_knex(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !crate::cmd::node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    emit("dependencies present");
    Ok(RunOutcome::Ran("knex available".into()))
}

fn knex_migrate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    spawn_cmd("npx", &["knex", "migrate:latest"], &ctx.cwd, emit)
}
