//! Prisma provider: detects prisma/schema.prisma and runs `prisma generate`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{spawn_cmd, which};

pub struct Prisma;

impl Provider for Prisma {
    fn id(&self) -> &'static str {
        "prisma"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["prisma/schema.prisma"]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if cwd.join("prisma").join("schema.prisma").is_file() {
            Some(self.found("prisma/schema.prisma"))
        } else {
            None
        }
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        let mut check = Task::new(
            "prisma-check",
            "check prisma available",
            "checks dependencies and the prisma CLI",
        )
        .risk(Risk::Low)
        .run(check_prisma);

        let mut gen = Task::new(
            "prisma-generate",
            "prisma generate",
            "generates the Prisma client from the schema (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["prisma-check"])
        .run(prisma_generate);

        if let Some(install) = crate::cmd::js_install_task(&ctx.cwd) {
            check = check.depends_on([install]);
            gen = gen.depends_on(["prisma-check", install]);
        }

        planner.add(check);
        planner.add(gen);
    }
}

fn check_prisma(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !crate::cmd::node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    emit("dependencies present");
    Ok(RunOutcome::Ran("prisma available".into()))
}

fn prisma_generate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    spawn_cmd("npx", &["prisma", "generate"], &ctx.cwd, emit)
}
