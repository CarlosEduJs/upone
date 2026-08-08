//! `MongoDB` provider: detects mongodb in docker-compose (or an `mongodb://`
//! URI in the env files) and ensures the service is up.
//!
//! When a compose file defines the service, the `docker` provider's
//! `docker compose up -d` is the single owner that starts it; this provider
//! only depends on that task and verifies the server responds. Without a
//! compose definition, upone cannot start mongodb for you, so it reports a
//! clear, actionable error instead of firing a broken command.

use std::path::Path;
use std::time::Duration;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::resolve_env_key;
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{compose_host_port, files_contain};

const COMPOSE_FILES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

const ENV_FILES: &[&str] = &[".env", ".env.local"];

const LOCAL_PORT: u16 = 27017;

pub struct Mongo;

impl Provider for Mongo {
    fn id(&self) -> &'static str {
        "mongo"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["mongod.conf"]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if files_contain(cwd, COMPOSE_FILES, &["mongo", "mongodb"]) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "docker-compose (mongo service)".into(),
                reason: "mongodb detected in docker-compose".into(),
            });
        }
        if files_contain(
            cwd,
            ENV_FILES,
            &["mongodb://", "mongodb+srv://", "MONGODB_URI=", "MONGO_URI="],
        ) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: ".env (mongodb URI)".into(),
                reason: "mongodb detected via env URI".into(),
            });
        }
        None
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        if files_contain(&ctx.cwd, COMPOSE_FILES, &["mongo", "mongodb"]) {
            planner.add(
                Task::new(
                    "mongo-up",
                    "verify mongodb is running",
                    "checks that mongodb responds after the compose service starts",
                )
                .risk(Risk::Low)
                .depends_on(["docker-up"])
                .run(mongo_verify),
            );
        } else {
            planner.add(
                Task::new(
                    "mongo-up",
                    "check mongodb is running",
                    "checks that mongodb responds on localhost:27017",
                )
                .risk(Risk::Low)
                .run(mongo_check),
            );
        }
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
        let mut checks = vec![ReadinessCheck::new(
            "mongo-tcp",
            format!("mongodb (localhost:{port})"),
            "MongoDB is accepting TCP connections",
            Importance::Required,
            move |_ctx| {
                if mongo_reachable(port) {
                    ReadinessStatus::Ready(format!("responding on localhost:{port}"))
                } else {
                    ReadinessStatus::NotReady {
                        reason: format!("mongodb not responding on localhost:{port}"),
                        remedy: "Run 'docker compose up -d' or check if the mongodb container is running".into(),
                    }
                }
            },
        )];

        let cwd = ctx.cwd.clone();
        checks.push(ReadinessCheck::new(
            "mongo-uri-env",
            "MONGODB_URI / MONGO_URI / DATABASE_URL",
            "a mongodb connection string is set",
            Importance::Required,
            move |_ctx| {
                if mongo_uri_env(&cwd).is_some() {
                    ReadinessStatus::Ready("found".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "MONGODB_URI, MONGO_URI or DATABASE_URL (mongodb://) not found in process env or .env* files".into(),
                        remedy: "Set MONGODB_URI (or MONGO_URI / DATABASE_URL with a mongodb:// URL) in your .env.local or shell environment".into(),
                    }
                }
            },
        ));

        checks
    }
}

/// Resolves the first mongodb connection string from the process env or
/// `.env*` files, checking `MONGODB_URI`, `MONGO_URI` and `DATABASE_URL`.
fn mongo_uri_env(cwd: &Path) -> Option<String> {
    for key in ["MONGODB_URI", "MONGO_URI", "DATABASE_URL"] {
        if let Some(val) = resolve_env_key(cwd, key) {
            if val.contains("mongodb") {
                return Some(val);
            }
        }
    }
    None
}

fn mongo_reachable(port: u16) -> bool {
    use std::net::TcpStream;
    let Ok(addr) = format!("127.0.0.1:{port}").parse() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn mongo_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
    if mongo_reachable(port) {
        emit(&format!("mongodb responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("mongodb already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "mongodb not responding on localhost:{port} after the compose services started. Check `docker compose up -d` / `docker compose logs mongodb`."
            ),
        ))
    }
}

/// No compose definition: nothing here can start mongodb, so it reports clearly.
fn mongo_check(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
    if mongo_reachable(port) {
        emit(&format!("mongodb responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("mongodb already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "mongodb is not responding on localhost:{port} and there is no docker-compose service to start it. \
                Start it yourself (e.g. `docker run -d -p {port}:27017 mongo`), then re-run upone."
            ),
        ))
    }
}
