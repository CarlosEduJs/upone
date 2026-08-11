//! Prisma provider: detects prisma/schema.prisma and runs `prisma generate`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{add_migration_plan, spawn_cmd, which, DbWiring, InstallKind};

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
        let check = Task::new(
            "prisma-check",
            "check prisma available",
            "checks dependencies and the prisma CLI",
        )
        .risk(Risk::Low)
        .run(check_prisma);

        let gen = Task::new(
            "prisma-generate",
            "prisma generate",
            "generates the Prisma client from the schema (safe to repeat)",
        )
        .risk(Risk::Medium)
        .run(prisma_generate);

        add_migration_plan(planner, ctx, check, gen, InstallKind::Js, DbWiring::None);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        let cwd = ctx.cwd.clone();
        let (output_dir, display_rel) = resolve_prisma_output_dir(&cwd);

        vec![ReadinessCheck::new(
            "prisma-client",
            "prisma client generated",
            format!("Prisma client exists in {display_rel}"),
            Importance::Required,
            move |_ctx| {
                let has_marker = output_dir.join("index.js").is_file()
                    || output_dir.join("index.d.ts").is_file()
                    || output_dir.join("index.mjs").is_file();
                if has_marker {
                    ReadinessStatus::Ready("prisma client present".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: format!("prisma client not found in {display_rel}"),
                        remedy: "Run 'npx prisma generate' or 'upone up'".into(),
                    }
                }
            },
        )]
    }
}

/// Resolves the Prisma client output directory from `prisma/schema.prisma`
/// if specified via `output = "..."`, otherwise defaults to `node_modules/.prisma/client`.
fn resolve_prisma_output_dir(cwd: &Path) -> (std::path::PathBuf, String) {
    let schema_path = cwd.join("prisma").join("schema.prisma");
    if let Ok(content) = std::fs::read_to_string(&schema_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("output") {
                if let Some(eq) = trimmed.find('=') {
                    let val = trimmed[eq + 1..].trim();
                    let raw_val = if (val.starts_with('"') && val.ends_with('"'))
                        || (val.starts_with('\'') && val.ends_with('\''))
                    {
                        &val[1..val.len() - 1]
                    } else {
                        val
                    };
                    if !raw_val.is_empty() {
                        let schema_dir = cwd.join("prisma");
                        let resolved = schema_dir.join(raw_val);
                        let rel = resolved
                            .strip_prefix(cwd)
                            .map_or_else(|_| raw_val.to_string(), |p| p.display().to_string());
                        return (resolved, rel);
                    }
                }
            }
        }
    }
    (
        cwd.join("node_modules/.prisma/client"),
        "node_modules/.prisma/client".to_string(),
    )
}

fn check_prisma(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !crate::cmd::node_modules_present(&ctx.cwd) {
        return Err(RunError::Failed(
            "node_modules missing. Install the project dependencies first (e.g. `upone up` installs them, or run your package manager's install).".into(),
        ));
    }
    emit("dependencies present");
    Ok(RunOutcome::Ran)
}

fn prisma_generate(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("npx") {
        return Err(RunError::Failed("npx not found on PATH".into()));
    }
    spawn_cmd("npx", &["prisma", "generate"], &ctx.cwd, emit)
}
