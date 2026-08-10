//! yarn provider: detects yarn.lock and runs `yarn install`.
//!
//! Supports both modern yarn (berry, Yarn 2+) and classic (v1): berry is
//! detected by its `.yarnrc.yml` / `PnP` markers and installs with
//! `--immutable`, classic uses `--frozen-lockfile` for reproducible installs.

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::ReadinessCheck;
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{any_exists, check_binary, spawn_cmd};

pub struct Yarn;

const BERRY_MARKERS: &[&str] = &[
    ".yarnrc.yml",
    ".yarnrc.yaml",
    ".pnp.cjs",
    ".pnp.js",
    ".pnp.loader.mjs",
];

impl Provider for Yarn {
    fn id(&self) -> &'static str {
        "yarn"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["yarn.lock"]
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "yarn-check",
            "check yarn installed",
            "checks that yarn is on PATH",
        )
        .risk(Risk::Low)
        .run(|_ctx, emit| {
            check_binary(
                "yarn",
                "Install it via `npm install -g yarn` or enable it with `corepack enable`.",
                emit,
            )
        });

        let install = Task::new(
            "yarn-install",
            "yarn install",
            "installs project dependencies with yarn (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["yarn-check"])
        .run(yarn_install);

        planner.add(check);
        planner.add(install);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        vec![crate::cmd::node_modules_check(
            "yarn-deps",
            "yarn",
            "Run 'yarn install' or 'upone up'",
            &ctx.cwd,
        )]
    }
}

fn yarn_install(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if ctx.cwd.join("node_modules").is_dir() {
        emit("node_modules already present, skipping install");
        return Ok(RunOutcome::Skipped("node_modules present".into()));
    }
    let args: &[&str] = if any_exists(&ctx.cwd, BERRY_MARKERS) {
        // Yarn 2+ (berry): --frozen-lockfile was removed; --immutable is the
        // non-updating equivalent.
        &["install", "--immutable"]
    } else {
        &["install", "--frozen-lockfile"]
    };
    spawn_cmd("yarn", args, &ctx.cwd, emit)
}
