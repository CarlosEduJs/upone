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

use crate::cmd::{compose_host_port, files_contain};

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
        if files_contain(&ctx.cwd, COMPOSE_FILES, &["postgres", "postgresql"]) {
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

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::*;

        let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 5432);
        let mut checks = vec![ReadinessCheck::new(
            "postgres-tcp",
            format!("postgres (localhost:{})", port),
            "PostgreSQL is accepting TCP connections",
            Importance::Required,
            move |_ctx| {
                if postgres_reachable(port) {
                    ReadinessStatus::Ready(format!("responding on localhost:{}", port))
                } else {
                    ReadinessStatus::NotReady {
                        reason: format!("postgres not responding on localhost:{}", port),
                        remedy: "Run 'docker compose up -d' or check if the postgres container is running".into(),
                    }
                }
            },
        )];

        // DATABASE_URL env key check.
        let cwd = ctx.cwd.clone();
        checks.push(ReadinessCheck::new(
            "env-DATABASE_URL",
            "DATABASE_URL",
            "DATABASE_URL environment variable is set",
            Importance::Required,
            move |_ctx| {
                if resolve_env_key(&cwd, "DATABASE_URL").is_some() {
                    ReadinessStatus::Ready("found".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "DATABASE_URL not found in process env or .env* files".into(),
                        remedy: "Add DATABASE_URL to your .env.local or shell environment".into(),
                    }
                }
            },
        ));

        checks
    }
}

fn postgres_reachable(port: u16) -> bool {
    use std::net::TcpStream;
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(300),
    )
    .is_ok()
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn postgres_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 5432);
    if postgres_reachable(port) {
        emit(&format!("postgres responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("postgres already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "postgres not responding on localhost:{port} after the compose services started. Check `docker compose up -d` / `docker compose logs postgres`."
            ),
        ))
    }
}

/// No compose definition: nothing here can start postgres, so it reports clearly.
fn postgres_check(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 5432);
    if postgres_reachable(port) {
        emit(&format!("postgres responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("postgres already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "postgres is not responding on localhost:{port} and there is no docker-compose service to start it. \
                Start it yourself (e.g. `docker run -d -p {port}:5432 -e POSTGRES_PASSWORD=postgres postgres`), then re-run upone."
            ),
        ))
    }
}
