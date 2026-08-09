//! `uv` provider: detects uv.lock and runs `uv sync`.

use upone_core::detect::Provider;
use upone_core::plan::{Planner, Task};
use upone_core::readiness::ReadinessCheck;
use upone_core::{Context, Risk};

use crate::cmd::{check_binary, spawn_cmd};

use super::python;

pub struct Uv;

impl Provider for Uv {
    fn id(&self) -> &'static str {
        "uv"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["uv.lock"]
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "uv-check",
            "check uv installed",
            "checks that uv is on PATH",
        )
        .risk(Risk::Low)
        .run(|_ctx, emit| {
            check_binary(
                "uv",
                "Install it via https://docs.astral.sh/uv/ or your system installer.",
                emit,
            )
        });

        let sync = Task::new(
            "uv-sync",
            "uv sync",
            "creates the project venv and installs dependencies with uv (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["uv-check"])
        .run(|ctx, emit| spawn_cmd("uv", &["sync"], &ctx.cwd, emit));

        planner.add(check);
        planner.add(sync);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        vec![python::venv_check(
            "uv-venv",
            ".venv exists (uv sync ran)",
            "Run 'uv sync' or 'upone up'",
            &ctx.cwd,
        )]
    }
}
