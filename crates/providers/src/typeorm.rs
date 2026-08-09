//! `typeorm` provider: detects the `TypeORM` ORM via the `typeorm` dependency or
//! a `data-source.*` / `ormconfig.*` file, and runs `typeorm migration:run`.
//!
//! `TypeORM` migrations apply the schema to the configured database, so the task
//! only runs after the project deps are installed and — when a database
//! provider reported it — after that DB is up.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{js_install_task, migration_db_dep, package_has_dependency, spawn_cmd, which};

const DATA_SOURCE_FILES: &[&str] = &[
    "data-source.ts",
    "data-source.js",
    "data-source.mts",
    "data-source.cts",
    "ormconfig.json",
    "ormconfig.ts",
    "ormconfig.js",
];

pub struct Typeorm;

impl Provider for Typeorm {
    fn id(&self) -> &'static str {
        "typeorm"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if package_has_dependency(cwd, "typeorm") {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "package.json (typeorm)".into(),
                reason: "TypeORM ORM detected".into(),
            });
        }
        for file in DATA_SOURCE_FILES {
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
            "typeorm-check",
            "check typeorm available",
            "checks dependencies and the typeorm CLI",
        )
        .risk(Risk::Low)
        .run(check_typeorm);

        let mut migrate = Task::new(
            "typeorm-migrate",
            "typeorm migration:run",
            "applies pending typeorm migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .depends_on(["typeorm-check"])
        .run(typeorm_migrate);

        if let Some(install) = js_install_task(&ctx.cwd) {
            check = check.depends_on([install]);
            migrate = migrate.depends_on(["typeorm-check", install]);
        }
        if let Some(db) = migration_db_dep(&ctx.cwd) {
            migrate = migrate.depends_on(["typeorm-check", db]);
        }

        planner.add(check);
        planner.add(migrate);
    }
}

fn check_typeorm(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !crate::cmd::node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    emit("dependencies present");
    Ok(RunOutcome::Ran("typeorm available".into()))
}

fn typeorm_migrate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    // Modern TypeORM requires the data source via `-d`; the legacy ormconfig
    // path needs no flag. Prefer the data source form when we can see one.
    let data_source = DATA_SOURCE_FILES.iter().find(|f| ctx.cwd.join(f).is_file());
    if let Some(file) = data_source {
        spawn_cmd(
            "npx",
            &["typeorm", "migration:run", "-d", file],
            &ctx.cwd,
            emit,
        )
    } else {
        spawn_cmd("npx", &["typeorm", "migration:run"], &ctx.cwd, emit)
    }
}
