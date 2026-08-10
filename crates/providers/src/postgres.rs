//! `PostgreSQL` provider: detects postgres in docker-compose (or `DATABASE_URL`)
//! and ensures the service is up.
//!
//! When a compose file defines the service, the `docker` provider's
//! `docker compose up -d` is the single owner that starts it; this provider
//! only depends on that task and verifies the server responds. Without a
//! compose definition, upone cannot start postgres for you, so it reports a
//! clear, actionable error instead of firing a broken `docker compose up`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{compose_host_port, env_key_check, files_contain};

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
        // Detected by content (docker-compose/DATABASE_URL), not by file signatures.
        &[]
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
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 5432);
        let mut checks = vec![ReadinessCheck::new(
            "postgres-tcp",
            format!("postgres (localhost:{port})"),
            "PostgreSQL is accepting TCP connections",
            Importance::Required,
            // Single-attempt probe: readiness must be immediate, unlike the
            // post-run verify task after docker-up which retries until the
            // freshly-started container finishes booting.
            move |_ctx| {
                if pg_accepting_with_deadline(port, std::time::Instant::now()) {
                    ReadinessStatus::Ready(format!("responding on localhost:{port}"))
                } else {
                    ReadinessStatus::NotReady {
                        reason: format!("postgres not responding on localhost:{port}"),
                        remedy: "Run 'docker compose up -d' or check if the postgres container is running".into(),
                    }
                }
            },
        )];

        // Require DATABASE_URL only when detection came from it — a compose
        // definition (or an inline ORM config) already pins the connection
        // target, so ready-ness can be satisfied by compose/services alone.
        if !files_contain(&ctx.cwd, COMPOSE_FILES, &["postgres", "postgresql"]) {
            check_env_key(&ctx.cwd, &mut checks);
        }
        checks
    }
}

/// Reports whether postgres on `127.0.0.1:port` actually accepts an application
/// connection, retrying for a few seconds.
///
/// A bare TCP probe answers `true` the moment docker's port proxy binds, before
/// the server inside the (freshly-started) container is listening, which lets
/// a migration task race the container. To be sure the server is really up we
/// complete a `PostgreSQL` startup (protocol 3.0) exchange: the server replies
/// with a packet (`R` auth request, `Z` ready-for-query, `E` error, ...) as
/// soon as it reaches the auth stage, so *any* bytes back mean it's accepting.
fn pg_accepting(port: u16) -> bool {
    pg_accepting_with_deadline(
        port,
        std::time::Instant::now() + std::time::Duration::from_secs(10),
    )
}

/// Like [`pg_accepting`] but bounded by `deadline`; the readiness path passes
/// an already-elapsed deadline so it makes exactly one attempt.
fn pg_accepting_with_deadline(port: u16, deadline: std::time::Instant) -> bool {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};

    let mut startup = Vec::with_capacity(64);
    startup.extend_from_slice(&[0u8; 4]); // message length, patched below
    startup.extend_from_slice(&196_608_u32.to_be_bytes()); // protocol 3.0
    for (key, value) in [("user", "upone"), ("database", "upone")] {
        startup.extend_from_slice(key.as_bytes());
        startup.push(0);
        startup.extend_from_slice(value.as_bytes());
        startup.push(0);
    }
    startup.push(0); // terminator
    let len = u32::try_from(startup.len()).unwrap_or(u32::MAX);
    startup[..4].copy_from_slice(&len.to_be_bytes());

    loop {
        let Ok(addrs) = ("127.0.0.1", port).to_socket_addrs() else {
            return false;
        };
        for addr in addrs {
            let Ok(mut s) =
                TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(500))
            else {
                continue;
            };
            let Ok(mut read) = s.try_clone() else {
                continue;
            };
            if s.set_read_timeout(Some(std::time::Duration::from_millis(750)))
                .is_err()
            {
                continue;
            }
            let mut buf = [0u8; 32];
            // A positive byte count from the server is required — a clean
            // EOF (`Ok(0)`) or a read error means it is not accepting yet.
            if s.write_all(&startup).is_ok() && read.read(&mut buf).is_ok_and(|n| n > 0) {
                return true;
            }
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// Adds the `env-DATABASE_URL` readiness check.
fn check_env_key(cwd: &Path, checks: &mut Vec<upone_core::readiness::ReadinessCheck>) {
    checks.push(env_key_check("env-DATABASE_URL", "DATABASE_URL", cwd));
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn postgres_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 5432);
    if pg_accepting(port) {
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
    if pg_accepting(port) {
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
