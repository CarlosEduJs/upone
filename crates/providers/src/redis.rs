//! Redis provider: detects redis in docker-compose (or redis.conf) and
//! ensures the service is up.
//!
//! When a compose file defines the service, the `docker` provider's
//! `docker compose up -d` is the single owner that starts it; this provider
//! only depends on that task and verifies the server responds. Without a
//! compose definition, upone cannot start redis for you, so it reports a
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

pub struct Redis;

impl Provider for Redis {
    fn id(&self) -> &'static str {
        "redis"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["redis.conf", "redis/sentinel.conf"]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if files_contain(cwd, COMPOSE_FILES, &["redis", "redislabs/redis"]) {
            return Some(upone_core::Detection {
                provider: "redis",
                signature: "docker-compose (redis service)".into(),
                reason: "redis detected in docker-compose".into(),
            });
        }
        for sig in self.signatures() {
            if cwd.join(sig).is_file() {
                return Some(self.found(sig));
            }
        }
        None
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        if any_exists(&ctx.cwd, COMPOSE_FILES) {
            planner.add(
                Task::new(
                    "redis-up",
                    "verify redis is running",
                    "checks that redis responds after the compose service starts",
                )
                .risk(Risk::Low)
                .depends_on(["docker-up"])
                .run(redis_verify),
            );
        } else {
            planner.add(
                Task::new(
                    "redis-up",
                    "check redis is running",
                    "checks that redis responds on localhost:6379",
                )
                .risk(Risk::Low)
                .run(redis_check),
            );
        }
    }
}

fn redis_reachable() -> bool {
    use std::net::TcpStream;
    TcpStream::connect_timeout(
        &"127.0.0.1:6379".parse().unwrap(),
        Duration::from_millis(300),
    )
    .is_ok()
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn redis_verify(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if redis_reachable() {
        emit("redis responding on localhost:6379");
        Ok(RunOutcome::Skipped("redis already up".into()))
    } else {
        Err(RunError::Failed(
            "redis not responding on localhost:6379 after the compose services started. Check `docker compose up -d` / `docker compose logs redis`.".into(),
        ))
    }
}

/// No compose definition: nothing here can start redis, so it reports clearly.
fn redis_check(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if redis_reachable() {
        emit("redis responding on localhost:6379");
        Ok(RunOutcome::Skipped("redis already up".into()))
    } else {
        Err(RunError::Failed(
            "redis is not responding on localhost:6379 and there is no docker-compose service to start it. \
            Start it yourself (e.g. `docker run -d -p 6379:6379 redis`), then re-run upone.".into(),
        ))
    }
}