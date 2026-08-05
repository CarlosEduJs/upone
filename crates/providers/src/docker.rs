//! Docker provider: detects docker-compose.yml and brings up services.

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{any_exists, spawn_cmd, which};

pub struct Docker;

impl Provider for Docker {
    fn id(&self) -> &'static str {
        "docker"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ]
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "docker-check",
            "check docker installed",
            "checks that docker is on PATH and the daemon responds",
        )
        .risk(Risk::Low)
        .run(check_docker);

        let up = Task::new(
            "docker-up",
            "docker compose up",
            "brings up the docker-compose services in the background (safe to repeat)",
        )
        .risk(Risk::High)
        .depends_on(["docker-check"])
        .run(docker_up);

        planner.add(check);
        planner.add(up);
    }
}

fn check_docker(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("docker") {
        return Err(RunError::Failed(
            "docker not found on PATH. Install it via https://docs.docker.com/get-docker/".into(),
        ));
    }
    emit("docker found on PATH");
    match std::process::Command::new("docker").args(["info"]).output() {
        Ok(o) if o.status.success() => Ok(RunOutcome::Ran("docker daemon responding".into())),
        Ok(_) => Err(RunError::Failed(
            "docker installed but the daemon is not running. Start Docker Desktop/daemon and try again.".into(),
        )),
        Err(e) => Err(RunError::Failed(format!("failed to query docker: {e}"))),
    }
}

fn docker_up(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !any_exists(
        &ctx.cwd,
        &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ],
    ) {
        return Ok(RunOutcome::Skipped("no compose file in the project".into()));
    }
    spawn_cmd("docker", &["compose", "up", "-d"], &ctx.cwd, emit)
}
