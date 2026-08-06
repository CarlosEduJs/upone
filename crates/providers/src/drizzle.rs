//! Drizzle provider: detects drizzle.config and runs `drizzle-kit generate`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{spawn_cmd, which};

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
        let mut check = Task::new(
            "drizzle-check",
            "check drizzle-kit available",
            "checks dependencies and the drizzle CLI",
        )
        .risk(Risk::Low)
        .run(check_drizzle);

        let mut gen = Task::new(
            "drizzle-generate",
            "drizzle-kit generate",
            "generates migrations from the schema (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["drizzle-check"])
        .run(drizzle_generate);

        if let Some(install) = crate::cmd::js_install_task(&ctx.cwd) {
            check = check.depends_on([install]);
            gen = gen.depends_on(["drizzle-check", install]);
        }

        planner.add(check);
        planner.add(gen);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        let cwd = ctx.cwd.clone();
        vec![ReadinessCheck::new(
            "drizzle-deps",
            "drizzle dependencies installed",
            "node_modules present for drizzle-kit",
            Importance::Required,
            move |_ctx| {
                if crate::cmd::node_modules_present(&cwd) {
                    ReadinessStatus::Ready("node_modules present".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "node_modules missing for drizzle".into(),
                        remedy: "Run your package manager's install or 'upone up'".into(),
                    }
                }
            },
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
    Ok(RunOutcome::Ran("drizzle-kit available".into()))
}

fn drizzle_generate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    spawn_cmd("npx", &["drizzle-kit", "generate"], &ctx.cwd, emit)
}
