//! Shared logic for JS package managers (npm/pnpm/bun).

use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{spawn_cmd, which};

/// Metadata for a JS package manager.
pub struct JsPm {
    pub id: &'static str,
    pub label: &'static str,
    /// Binary (ex: `"npm"`, `"pnpm"`, `"bun"`).
    pub bin: &'static str,
    /// Lockfile signatures that detect the project.
    pub signatures: &'static [&'static str],
    /// Extra `install` args (ex: `["--frozen-lockfile"]`).
    pub install_args: &'static [&'static str],
}

fn check(
    pm: &'static JsPm,
    _ctx: &Context,
    emit: &mut dyn FnMut(&str),
) -> Result<RunOutcome, RunError> {
    if which(pm.bin) {
        emit(&format!("{} found on PATH", pm.bin));
        Ok(RunOutcome::Ran(format!("{} installed", pm.bin)))
    } else {
        Err(RunError::Failed(format!(
            "{} not found on PATH. Install it via https://nodejs.org or your system's installer.",
            pm.bin
        )))
    }
}

fn install(
    pm: &'static JsPm,
    ctx: &Context,
    emit: &mut dyn FnMut(&str),
) -> Result<RunOutcome, RunError> {
    if ctx.cwd.join("node_modules").is_dir() {
        emit("node_modules already exists, skipping install");
        return Ok(RunOutcome::Skipped("node_modules present".into()));
    }
    let mut args: Vec<&str> = vec!["install"];
    args.extend_from_slice(pm.install_args);
    spawn_cmd(pm.bin, &args, &ctx.cwd, emit)
}

/// Adds `check` + `install` for a package manager to the plan.
pub fn add_install_plan(pm: &'static JsPm, _ctx: &Context, planner: &mut Planner<'_>) {
    let check = Task::new(
        format!("{}-check", pm.id),
        format!("check {} installed", pm.id),
        format!("checks that {} is on PATH", pm.id),
    )
    .risk(Risk::Low)
    .run(move |ctx, emit| check(pm, ctx, emit));

    let install = Task::new(
        format!("{}-install", pm.id),
        format!("{} install", pm.label),
        format!("installs project dependencies with {}", pm.label),
    )
    .risk(Risk::Medium)
    .depends_on([format!("{}-check", pm.id)])
    .run(move |ctx, emit| install(pm, ctx, emit));

    planner.add(check);
    planner.add(install);
}
