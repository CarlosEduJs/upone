//! Redis provider: detects redis in docker-compose (or a `redis://` /
//! `rediss://` URL) and ensures the service is up.
//!
//! When a compose file defines the service, the `docker` provider's
//! `docker compose up -d` is the single owner that starts it; this provider
//! only depends on that task and verifies the server responds. Without a
//! compose definition, upone cannot start redis for you, so it reports a
//! clear, actionable error instead of firing a broken `docker compose up`.
//! A configured `REDIS_URL` is validated against its own target — it is
//! never started.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::resolve_env_key;
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{compose_host_port, files_contain, parse_uri_authority, tcp_reachable};

const COMPOSE_FILES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

const ENV_FILES: &[&str] = &[".env", ".env.local"];

/// Env keys that may hold a redis connection URL.
const REDIS_URL_KEYS: &[&str] = &["REDIS_URL", "DATABASE_URL"];

const LOCAL_PORT: u16 = 6379;

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
        if env_has_redis_url(cwd) {
            return Some(upone_core::Detection {
                provider: "redis",
                signature: ".env (REDIS_URL redis)".into(),
                reason: "redis detected via REDIS_URL".into(),
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
        if files_contain(&ctx.cwd, COMPOSE_FILES, &["redis", "redislabs/redis"]) {
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
        } else if redis_uri(&ctx.cwd).is_some() {
            planner.add(
                Task::new(
                    "redis-up",
                    "verify redis URI",
                    "validates the configured redis target; externally managed URIs are only verified, never started",
                )
                .risk(Risk::Low)
                .run(redis_uri_verify),
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

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        let cwd = ctx.cwd.clone();
        let mut checks = Vec::new();

        if files_contain(&ctx.cwd, COMPOSE_FILES, &["redis", "redislabs/redis"]) {
            let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 6379);
            checks.push(ReadinessCheck::new(
                "redis-tcp",
                format!("redis (localhost:{port})"),
                "Redis is accepting TCP connections",
                Importance::Required,
                move |_ctx| {
                    if tcp_reachable("127.0.0.1", port) {
                        ReadinessStatus::Ready(format!("responding on localhost:{port}"))
                    } else {
                        ReadinessStatus::NotReady {
                            reason: format!("redis not responding on localhost:{port}"),
                            remedy: "Run 'docker compose up -d' or start redis manually".into(),
                        }
                    }
                },
            ));
        } else if let Some(uri) = redis_uri(&cwd) {
            checks.push(ReadinessCheck::new(
                "redis-tcp",
                "redis (configured URI)",
                "Redis target from the configured URI is reachable",
                Importance::Required,
                move |_ctx| match parse_uri_authority(&uri, &["redis", "rediss"], LOCAL_PORT) {
                    Some((host, port)) if tcp_reachable(&host, port) => {
                        ReadinessStatus::Ready(format!("responding on {host}:{port}"))
                    }
                    Some((host, port)) => ReadinessStatus::NotReady {
                        reason: format!("redis target {host}:{port} not responding"),
                        remedy: "Check the REDIS_URL value and start redis".into(),
                    },
                    None => ReadinessStatus::NotReady {
                        reason: "could not parse a hostname from the redis URL".into(),
                        remedy: "Check the REDIS_URL / DATABASE_URL value".into(),
                    },
                },
            ));
            checks.push(ReadinessCheck::new(
                "redis-uri-env",
                "REDIS_URL",
                "a redis connection string is set",
                Importance::Required,
                move |_ctx| {
                    if redis_uri(&cwd).is_some() {
                        ReadinessStatus::Ready("found".into())
                    } else {
                        ReadinessStatus::NotReady {
                            reason: "REDIS_URL (or DATABASE_URL with a redis:// URL) not found in process env or .env* files".into(),
                            remedy: "Set REDIS_URL in your .env.local or shell environment".into(),
                        }
                    }
                },
            ));
        } else {
            checks.push(ReadinessCheck::new(
                "redis-tcp",
                "redis (localhost:6379)",
                "Redis is accepting TCP connections",
                Importance::Required,
                move |_ctx| {
                    if tcp_reachable("127.0.0.1", 6379) {
                        ReadinessStatus::Ready("responding on localhost:6379".into())
                    } else {
                        ReadinessStatus::NotReady {
                            reason: "redis not responding on localhost:6379".into(),
                            remedy: "Run 'docker compose up -d' or start redis manually".into(),
                        }
                    }
                },
            ));
        }

        checks
    }
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn redis_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 6379);
    if tcp_reachable("127.0.0.1", port) {
        emit(&format!("redis responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("redis already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "redis not responding on localhost:{port} after the compose services started. Check `docker compose up -d` / `docker compose logs redis`."
            ),
        ))
    }
}

/// Externally configured URI: validate its own target; there is nothing here
/// that can start an external redis.
fn redis_uri_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let Some(uri) = redis_uri(&ctx.cwd) else {
        return Err(RunError::Failed(
            "a redis URL was configured but is no longer resolvable".into(),
        ));
    };
    let Some((host, port)) = parse_uri_authority(&uri, &["redis", "rediss"], LOCAL_PORT) else {
        return Err(RunError::Failed(
            "could not parse a host:port from the REDIS_URL value".into(),
        ));
    };
    if tcp_reachable(&host, port) {
        emit(&format!("redis responding on {host}:{port}"));
        Ok(RunOutcome::Skipped(format!(
            "redis already up ({host}:{port})"
        )))
    } else {
        Err(RunError::Failed(format!(
            "redis is not responding on {host}:{port} and there is no docker-compose service to start it. \
            Start it yourself (e.g. `docker run -d -p {port}:6379 redis`), then re-run upone."
        )))
    }
}

/// No compose definition: nothing here can start redis, so it reports clearly.
fn redis_check(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, 6379);
    if tcp_reachable("127.0.0.1", port) {
        emit(&format!("redis responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("redis already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "redis is not responding on localhost:{port} and there is no docker-compose service to start it. \
                Start it yourself (e.g. `docker run -d -p {port}:6379 redis`), then re-run upone."
            ),
        ))
    }
}

/// True when `value` uses a redis connection scheme (`redis://` or
/// `rediss://`). A strict prefix check, so values that merely *contain* a
/// redis word (or use another scheme) are never accepted.
fn is_redis_uri(value: &str) -> bool {
    value.starts_with("redis://") || value.starts_with("rediss://")
}

/// Strips a pair of surrounding quotes from an env value, if present.
fn unquote(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Scans the env files for a `REDIS_URL`/`DATABASE_URL` whose value actually
/// uses a redis scheme (file-based, for detection).
fn env_has_redis_url(cwd: &Path) -> bool {
    ENV_FILES.iter().any(|file| {
        std::fs::read_to_string(cwd.join(file)).is_ok_and(|content| {
            content.lines().any(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return false;
                }
                let line = line.strip_prefix("export ").unwrap_or(line);
                let Some((key, value)) = line.split_once('=') else {
                    return false;
                };
                REDIS_URL_KEYS.contains(&key.trim()) && is_redis_uri(unquote(value.trim()))
            })
        })
    })
}

/// Resolves the first configured redis connection string from the process
/// env or `.env*` files, accepting only `redis://`/`rediss://` schemes.
fn redis_uri(cwd: &Path) -> Option<String> {
    for key in REDIS_URL_KEYS {
        if let Some(val) = resolve_env_key(cwd, key) {
            if is_redis_uri(&val) {
                return Some(val);
            }
        }
    }
    None
}
