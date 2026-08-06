//! PostgreSQL provider: detects postgres in docker-compose (or DATABASE_URL)
//! and ensures the service is up.
//!
//! When a compose file defines the service, the `docker` provider's
//! `docker compose up -d` is the single owner that starts it; this provider
//! only depends on that task and verifies the server responds. Without a
//! compose definition, upone cannot start postgres for you, so it reports a
//! clear, actionable error instead of firing a broken `docker compose up`.

use std::path::Path;
use std::time::Duration;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{any_exists, files_contain};

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

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        if any_exists(&ctx.cwd, COMPOSE_FILES) {
            planner.add(
                Task::new(
                    "postgres-up",
                    "verify postgres is running",
                    "checks that postgres responds after the compose service starts",
                )
                .risk(Risk::Low)
                .depends_on(["docker-up"])
                .run(postgres_verify),
            );
        } else {
            planner.add(
                Task::new(
                    "postgres-up",
                    "check postgres is running",
                    "checks that postgres responds on localhost:5432",
                )
                .risk(Risk::Low)
                .run(postgres_check),
            );
        }
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

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn postgres_verify(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if postgres_reachable() {
        emit("postgres responding on localhost:5432");
        Ok(RunOutcome::Skipped("postgres already up".into()))
    } else {
        Err(RunError::Failed(
            "postgres not responding on localhost:5432 after the compose services started. Check `docker compose up -d` / `docker compose logs postgres`.".into(),
        ))
    }
}

/// No compose definition: nothing here can start postgres, so it reports clearly.
fn postgres_check(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if postgres_reachable() {
        emit("postgres responding on localhost:5432");
        Ok(RunOutcome::Skipped("postgres already up".into()))
    } else {
        Err(RunError::Failed(
            "postgres is not responding on localhost:5432 and there is no docker-compose service to start it. \
            Start it yourself (e.g. `docker run -d -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres`), then re-run upone.".into(),
        ))
    }
}