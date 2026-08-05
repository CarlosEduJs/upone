//! PostgreSQL provider: detects postgres in docker-compose (or DATABASE_URL)
//! and ensures the service is up.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{files_contain, spawn_cmd, which};

const COMPOSE_FILES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

pub struct Postgres;

impl Provider for Postgres {
    fn id(&self) -> &'static str {
        "postgres"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["postgresql.conf", ".env"]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if files_contain(cwd, COMPOSE_FILES, &["postgres", "postgresql"]) {
            return Some(upone_core::Detection {
                provider: "postgres",
                signature: "docker-compose (postgres service)".into(),
                reason: "postgres detected in docker-compose".into(),
            });
        }
        if files_contain(
            cwd,
            &[".env", ".env.local"],
            &["DATABASE_URL=postgres", "postgres://"],
        ) {
            return Some(upone_core::Detection {
                provider: "postgres",
                signature: ".env (DATABASE_URL postgres)".into(),
                reason: "postgres detected via DATABASE_URL".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let up = Task::new(
            "postgres-up",
            "ensure postgres is up",
            "checks if postgres responds; if not, tries to start it via docker compose",
        )
        .risk(Risk::Medium)
        .run(postgres_ensure);

        planner.add(up);
    }
}

fn postgres_reachable() -> bool {
    use std::net::TcpStream;
    TcpStream::connect_timeout(
        &"127.0.0.1:5432".parse().unwrap(),
        Duration::from_millis(300),
    )
    .is_ok()
}

fn postgres_ensure(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if postgres_reachable() {
        emit("postgres responding on localhost:5432");
        return Ok(RunOutcome::Skipped("postgres already up".into()));
    }
    if !which("docker") {
        return Err(RunError::Failed(
            "postgres is not running on localhost:5432 and docker is not available. Start postgres (e.g. `docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres`) and try again.".into(),
        ));
    }
    emit("postgres not responding; starting via docker compose");
    spawn_cmd(
        "docker",
        &["compose", "up", "-d", "postgres"],
        &ctx.cwd,
        emit,
    )?;

    if postgres_reachable() {
        emit("postgres is now responding");
        Ok(RunOutcome::Ran("postgres is up".into()))
    } else {
        let _ = Command::new("docker").args(["compose", "ps"]).output();
        Err(RunError::Failed(
            "postgres still not responding after docker compose up. Make sure the service is named 'postgres' in the compose file and check the logs with `docker compose logs postgres`.".into(),
        ))
    }
}
