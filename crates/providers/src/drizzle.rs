//! Drizzle provider: detects drizzle.config and runs `drizzle-kit generate`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{add_migration_plan, spawn_cmd, which, DbWiring, InstallKind};

pub struct Drizzle;

impl Provider for Drizzle {
    fn id(&self) -> &'static str {
        "drizzle"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[
            "drizzle.config.ts",
            "drizzle.config.js",
            "drizzle.config.json",
            "drizzle.config.mts",
        ]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        for sig in self.signatures() {
            if cwd.join(sig).is_file() {
                return Some(self.found(sig));
            }
        }
        None
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "drizzle-check",
            "check drizzle-kit available",
            "checks dependencies and the drizzle CLI",
        )
        .risk(Risk::Low)
        .run(check_drizzle);

        let gen = Task::new(
            "drizzle-generate",
            "drizzle-kit generate",
            "generates migrations from the schema (safe to repeat)",
        )
        .risk(Risk::Medium)
        .run(drizzle_generate);

        add_migration_plan(planner, ctx, check, gen, InstallKind::Js, DbWiring::None);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        vec![crate::cmd::node_modules_check(
            "drizzle-deps",
            "drizzle",
            "Run your package manager's install or 'upone up'",
            &ctx.cwd,
        )]
    }
}

fn check_drizzle(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !crate::cmd::node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    emit("dependencies present");
    Ok(RunOutcome::Ran)
}

fn drizzle_generate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    spawn_cmd("npx", &["drizzle-kit", "generate"], &ctx.cwd, emit)
}
