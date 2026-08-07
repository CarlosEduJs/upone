//! Go provider: detects go.mod and runs `go mod tidy` + `go build ./...`.

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{spawn_cmd, which_probe};

pub struct Go;

impl Provider for Go {
    fn id(&self) -> &'static str {
        "go"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["go.mod"]
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "go-check",
            "check go installed",
            "checks that the go toolchain is on PATH",
        )
        .risk(Risk::Low)
        .run(check_go);

        let tidy = Task::new(
            "go-tidy",
            "go mod tidy",
            "resolves and syncs the module graph and go.sum",
        )
        .risk(Risk::Medium)
        .depends_on(["go-check"])
        .run(go_tidy);

        let build = Task::new(
            "go-build",
            "go build ./...",
            "builds the project and its dependencies (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["go-tidy"])
        .run(go_build);

        planner.add(check);
        planner.add(tidy);
        planner.add(build);
    }

    fn readiness_checks(&self, _ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        vec![ReadinessCheck::new(
            "go-toolchain",
            "go on PATH",
            "go toolchain is available",
            Importance::Required,
            |_ctx| {
                if which_probe("go", "version") {
                    ReadinessStatus::Ready("go found".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "go not found on PATH".into(),
                        remedy: "Install Go via https://go.dev/dl".into(),
                    }
                }
            },
        )]
    }
}

fn check_go(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if which_probe("go", "version") {
        emit("go found on PATH");
        Ok(RunOutcome::Ran("go installed".into()))
    } else {
        Err(RunError::Failed(
            "go not found on PATH. Install it via https://go.dev/dl".into(),
        ))
    }
}

fn go_tidy(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    spawn_cmd("go", &["mod", "tidy"], &ctx.cwd, emit)
}

fn go_build(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    spawn_cmd("go", &["build", "./..."], &ctx.cwd, emit)
}
