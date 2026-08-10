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

use crate::cmd::{
    add_migration_plan, local_cli, node_modules_present, package_has_dependency, spawn_cmd, which,
    DbWiring, InstallKind,
};

const DATA_SOURCE_FILES: &[&str] = &[
    "data-source.ts",
    "data-source.js",
    "data-source.mts",
    "data-source.cts",
];

/// Legacy ormconfig files. `TypeORM` still loads these implicitly (no `-d`), so
/// they're detected but never passed to the CLI's `-d` flag.
const ORMCONFIG_FILES: &[&str] = &[
    "ormconfig.json",
    "ormconfig.ts",
    "ormconfig.js",
    "ormconfig.yml",
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
        for file in DATA_SOURCE_FILES.iter().chain(ORMCONFIG_FILES) {
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
            "typeorm-check",
            "check typeorm available",
            "checks dependencies and the typeorm CLI",
        )
        .risk(Risk::Low)
        .run(check_typeorm);

        let migrate = Task::new(
            "typeorm-migrate",
            "typeorm migration:run",
            "applies pending typeorm migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .run(typeorm_migrate);

        add_migration_plan(
            planner,
            ctx,
            check,
            migrate,
            InstallKind::Js,
            DbWiring::Database,
        );
    }
}

fn check_typeorm(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    if !package_has_dependency(&ctx.cwd, "typeorm") {
        return Err(RunError::Failed(
            "typeorm is not declared in package.json. Add it as a dependency and run the install task before migrating.".into(),
        ));
    }
    if !local_cli(&ctx.cwd, "typeorm") {
        return Err(RunError::Failed(
            "typeorm is not installed locally. Install the project dependencies first, and ensure typeorm's CLI lands on node_modules/.bin (upone won't let npx fetch it from the registry on demand).".into(),
        ));
    }
    emit("dependencies present, local typeorm CLI found");
    Ok(RunOutcome::Ran("typeorm available".into()))
}

fn typeorm_migrate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    // Modern TypeORM loads a data source via `-d`. TS data sources run through
    // the ts-node TypeORM runtime shipped in the package; JS data sources use
    // the plain CLI. Legacy ormconfig.* never takes `-d` — TypeORM picks it up
    // implicitly, so pass no flag. With no file, run with a bare `migration:run`.
    if let Some(file) = DATA_SOURCE_FILES.iter().find(|f| ctx.cwd.join(f).is_file()) {
        let cli = match Path::new(file)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
        {
            Some("mts" | "cts") => "typeorm-ts-node-esm",
            Some("ts") => "typeorm-ts-node-commonjs",
            _ => "typeorm",
        };
        return spawn_cmd(
            "npx",
            &["--no-install", cli, "migration:run", "-d", file],
            &ctx.cwd,
            emit,
        );
    }
    spawn_cmd(
        "npx",
        &["--no-install", "typeorm", "migration:run"],
        &ctx.cwd,
        emit,
    )
}
