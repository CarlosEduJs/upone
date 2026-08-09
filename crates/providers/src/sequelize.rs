//! `sequelize` provider: detects the Sequelize ORM via the `sequelize`
//! dependency (or a sequelize-cli `config/` layout) and runs
//! `sequelize-cli db:migrate`.
//!
//! Migrations apply the schema to the configured database, so the task only
//! runs after deps are installed and, when reported, after the DB is up.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{
    js_install_task, js_managed, local_cli, migration_db_dep, package_has_dependency, spawn_cmd,
    which,
};

pub struct Sequelize;

impl Provider for Sequelize {
    fn id(&self) -> &'static str {
        "sequelize"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if js_managed(cwd) && package_has_dependency(cwd, "sequelize") {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "package.json (sequelize)".into(),
                reason: "Sequelize ORM detected".into(),
            });
        }
        // sequelize-cli default layout: config/ carries the connection config.
        if js_managed(cwd) && cwd.join("config").join("config.json").is_file() {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "sequelize-cli structure".into(),
                reason: "sequelize-cli config detected".into(),
            });
        }
        None
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        let mut check = Task::new(
            "sequelize-check",
            "check sequelize available",
            "checks that the local sequelize-cli binary is installed",
        )
        .risk(Risk::Low)
        .run(check_sequelize);

        let mut migrate = Task::new(
            "sequelize-migrate",
            "sequelize-cli db:migrate",
            "applies pending sequelize migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .depends_on(["sequelize-check"])
        .run(sequelize_migrate);

        if let Some(install) = js_install_task(&ctx.cwd) {
            check = check.depends_on([install]);
            migrate = migrate.depends_on(["sequelize-check", install]);
        }
        if let Some(db) = migration_db_dep(&ctx.cwd) {
            migrate = migrate.depends_on(["sequelize-check", db]);
        }

        planner.add(check);
        planner.add(migrate);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        let cwd = ctx.cwd.clone();
        vec![ReadinessCheck::new(
            "sequelize-deps",
            "sequelize dependencies installed",
            "node_modules present for sequelize",
            Importance::Required,
            move |_ctx| {
                if crate::cmd::node_modules_present(&cwd) {
                    ReadinessStatus::Ready("node_modules present".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "node_modules missing for sequelize".into(),
                        remedy: "Run your package manager's install or 'upone up'".into(),
                    }
                }
            },
        )]
    }
}

fn check_sequelize(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !crate::cmd::node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    if !local_cli(&ctx.cwd, "sequelize-cli") {
        return Err(RunError::Failed(
            "sequelize-cli is not installed locally. Add it as a devDependency and run the install task (a registry-installed npx binary would be fetched on demand, which upone avoids)".into(),
        ));
    }
    emit("dependencies present, sequelize-cli installed locally");
    Ok(RunOutcome::Ran("sequelize available".into()))
}

fn sequelize_migrate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    spawn_cmd(
        "npx",
        &["--no-install", "sequelize-cli", "db:migrate"],
        &ctx.cwd,
        emit,
    )
}
