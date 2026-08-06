//! Cargo provider: detects Cargo.toml and runs `cargo build`.

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{spawn_cmd, which};

pub struct Cargo;

impl Provider for Cargo {
    fn id(&self) -> &'static str {
        "cargo"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["Cargo.toml"]
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "cargo-check",
            "check cargo installed",
            "checks that cargo is on PATH",
        )
        .risk(Risk::Low)
        .run(check_cargo);

        let build = Task::new(
            "cargo-build",
            "cargo build",
            "builds the project and its dependencies (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["cargo-check"])
        .run(cargo_build);

        planner.add(check);
        planner.add(build);
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::*;

        let cwd = ctx.cwd.clone();
        vec![
            ReadinessCheck::new(
                "cargo-toolchain",
                "cargo on PATH",
                "cargo CLI is available",
                Importance::Required,
                |_ctx| {
                    if which("cargo") {
                        ReadinessStatus::Ready("cargo found".into())
                    } else {
                        ReadinessStatus::NotReady {
                            reason: "cargo not found on PATH".into(),
                            remedy: "Install Rust via rustup: https://rustup.rs".into(),
                        }
                    }
                },
            ),
            ReadinessCheck::new(
                "cargo-lock",
                "Cargo.lock present",
                "Cargo.lock exists (dependencies resolved)",
                Importance::Required,
                move |_ctx| {
                    if cwd.join("Cargo.lock").is_file() {
                        ReadinessStatus::Ready("Cargo.lock found".into())
                    } else {
                        ReadinessStatus::NotReady {
                            reason: "Cargo.lock not found".into(),
                            remedy: "Run 'cargo build' or 'cargo generate-lockfile' to generate it"
                                .into(),
                        }
                    }
                },
            ),
        ]
    }
}

fn check_cargo(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if which("cargo") {
        emit("cargo found on PATH");
        Ok(RunOutcome::Ran("cargo installed".into()))
    } else {
        Err(RunError::Failed(
            "cargo not found on PATH. Install Rust via rustup: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`".into(),
        ))
    }
}

fn cargo_build(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    // cargo build is idempotent by nature; uses incremental cache.
    spawn_cmd("cargo", &["build"], &ctx.cwd, emit)
}
