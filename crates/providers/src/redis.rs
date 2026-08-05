//! Redis provider: detects redis in docker-compose (or redis.conf) and
//! ensures the service is up.

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

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let up = Task::new(
            "redis-up",
            "ensure redis is up",
            "checks if redis responds; if not, tries to start it via docker compose",
        )
        .risk(Risk::Medium)
        .run(redis_ensure);

        planner.add(up);
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

fn redis_ensure(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if redis_reachable() {
        emit("redis responding on localhost:6379");
        return Ok(RunOutcome::Skipped("redis already up".into()));
    }
    if !which("docker") {
        return Err(RunError::Failed(
            "redis is not running on localhost:6379 and docker is not available. Start redis (e.g. `docker run -d -p 6379:6379 redis`) and try again.".into(),
        ));
    }
    emit("redis not responding; starting via docker compose");
    spawn_cmd("docker", &["compose", "up", "-d", "redis"], &ctx.cwd, emit)?;

    if redis_reachable() {
        emit("redis is now responding");
        Ok(RunOutcome::Ran("redis is up".into()))
    } else {
        let _ = Command::new("docker").args(["compose", "ps"]).output();
        Err(RunError::Failed(
            "redis still not responding after docker compose up. Make sure the service is named 'redis' in the compose file and check the logs with `docker compose logs redis`.".into(),
        ))
    }
}
