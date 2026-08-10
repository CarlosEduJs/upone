//! `knex` provider: detects the Knex query builder via the `knex` dependency
//! or a `knexfile.*` and runs `knex migrate:latest`.
//!
//! Migrations apply the schema to the configured database, so the task only
//! runs after deps are installed and, when reported, after the DB is up.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::ReadinessCheck;
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{
    add_migration_plan, js_managed, local_cli, node_modules_present, package_has_dependency,
    spawn_cmd, which, DbWiring, InstallKind,
};

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
        if js_managed(cwd) && package_has_dependency(cwd, "knex") {
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
        let check = Task::new(
            "knex-check",
            "check knex available",
            "checks that the local knex binary is installed",
        )
        .risk(Risk::Low)
        .run(check_knex);

        let migrate = Task::new(
            "knex-migrate",
            "knex migrate:latest",
            "applies pending knex migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .run(knex_migrate);

        add_migration_plan(
            planner,
            ctx,
            check,
            migrate,
            InstallKind::Js,
            DbWiring::Database,
        );
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        vec![crate::cmd::node_modules_check(
            "knex-deps",
            "knex",
            "Run your package manager's install or 'upone up'",
            &ctx.cwd,
        )]
    }
}

fn check_knex(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    if !local_cli(&ctx.cwd, "knex") {
        return Err(RunError::Failed(
            "knex is not installed locally. Add it as a dependency and run the install task (upone won't let npx fetch it from the registry on demand).".into(),
        ));
    }
    emit("dependencies present, knex CLI present");
    Ok(RunOutcome::Ran("knex available".into()))
}

fn knex_migrate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    spawn_cmd(
        "npx",
        &["--no-install", "knex", "migrate:latest"],
        &ctx.cwd,
        emit,
    )
}
