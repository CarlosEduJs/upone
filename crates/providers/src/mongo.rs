//! `MongoDB` provider: detects mongodb in docker-compose or a `mongodb://` /
//! `mongodb+srv://` URI and verifies the service is up.
//!
//! When a compose file defines the service, the `docker` provider's
//! `docker compose up -d` is the single owner that starts it; this provider
//! depends on that task and verifies the server responds. Without a compose
//! definition upone cannot start mongodb, so an externally configured URI is
//! only verified against its own target — it is never started.

use std::net::{TcpStream, ToSocketAddrs};
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

const MONGODB_KEYS: &[&str] = &["MONGODB_URI", "MONGO_URI", "DATABASE_URL"];

const LOCAL_PORT: u16 = 27017;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

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
        if env_has_mongodb_uri(cwd) {
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
        } else if mongo_uri(&ctx.cwd).is_some() {
            planner.add(
                Task::new(
                    "mongo-up",
                    "verify mongodb URI",
                    "validates the configured mongodb target; externally managed URIs are only verified, never started",
                )
                .risk(Risk::Low)
                .run(mongo_uri_verify),
            );
        }
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        let cwd = ctx.cwd.clone();
        let mut checks = Vec::new();

        if files_contain(&ctx.cwd, COMPOSE_FILES, &["mongo", "mongodb"]) {
            let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
            checks.push(ReadinessCheck::new(
                "mongo-tcp",
                format!("mongodb (localhost:{port})"),
                "MongoDB is accepting TCP connections",
                Importance::Required,
                move |_| {
                    if reachable("127.0.0.1", port) {
                        ReadinessStatus::Ready(format!("responding on localhost:{port}"))
                    } else {
                        ReadinessStatus::NotReady {
                            reason: format!("mongodb not responding on localhost:{port}"),
                            remedy: "Run 'docker compose up -d' or check if the mongodb container is running".into(),
                        }
                    }
                },
            ));
        } else if let Some(uri) = mongo_uri(&cwd) {
            checks.push(ReadinessCheck::new(
                "mongo-tcp",
                "mongodb (configured URI)",
                "MongoDB target from the configured URI is reachable",
                Importance::Required,
                move |_| match mongodb_target(&uri) {
                    Some((host, port)) if reachable(&host, port) => {
                        ReadinessStatus::Ready(format!("responding on {host}:{port}"))
                    }
                    Some((host, port)) => ReadinessStatus::NotReady {
                        reason: format!("mongodb target {host}:{port} not responding"),
                        remedy: "Check the mongodb service for the configured URI".into(),
                    },
                    None => ReadinessStatus::Ready(
                        "external mongodb+srv URI configured (no local server to check)".into(),
                    ),
                },
            ));
        }

        checks.push(ReadinessCheck::new(
            "mongo-uri-env",
            "MONGODB_URI / MONGO_URI / DATABASE_URL",
            "a mongodb connection string is set",
            Importance::Required,
            move |_| {
                if mongo_uri(&cwd).is_some() {
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

/// True when `value` uses a `MongoDB` connection scheme (`mongodb://` or
/// `mongodb+srv://`). Values that merely *contain* the word (e.g. a path like
/// `mongodb://` inside a badge URL) are rejected, so this is a strict prefix
/// check. Shared by detection and readiness so that values that
/// merely contain "mongodb" (or use another scheme) are never accepted.
fn is_mongodb_uri(value: &str) -> bool {
    value.starts_with("mongodb://") || value.starts_with("mongodb+srv://")
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

/// Scans the env files for a `MONGODB_URI`/`MONGO_URI`/`DATABASE_URL` whose
/// value actually uses a mongodb scheme (file-based, for detection).
fn env_has_mongodb_uri(cwd: &Path) -> bool {
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
                MONGODB_KEYS.contains(&key.trim()) && is_mongodb_uri(unquote(value.trim()))
            })
        })
    })
}

/// Resolves the first configured mongodb connection string from the process
/// env or `.env*` files, accepting only `mongodb://`/`mongodb+srv://` schemes.
fn mongo_uri(cwd: &Path) -> Option<String> {
    for key in MONGODB_KEYS {
        if let Some(val) = resolve_env_key(cwd, key) {
            if is_mongodb_uri(&val) {
                return Some(val);
            }
        }
    }
    None
}

/// Extracts the `host:port` target of a `mongodb://` URI, dropping any
/// credentials and database part. Returns `None` for `mongodb+srv://` URIs
/// (no single directly-connectable target) or unparseable authorities.
fn mongodb_target(uri: &str) -> Option<(String, u16)> {
    let authority = uri.strip_prefix("mongodb://")?.split('/').next()?;
    let authority = authority.rsplit('@').next()?;
    if authority.is_empty() {
        return None;
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok() => {
            Some((host.to_string(), port.parse::<u16>().ok()?))
        }
        _ => Some((authority.to_string(), LOCAL_PORT)),
    }
}

fn reachable(host: &str, port: u16) -> bool {
    format!("{host}:{port}")
        .to_socket_addrs()
        .is_ok_and(|mut addrs| {
            addrs
                .find_map(|addr| TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok())
                .is_some()
        })
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn mongo_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
    if reachable("127.0.0.1", port) {
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

/// Externally configured URI: validate its own target; there is nothing here
/// that can start an external mongodb.
fn mongo_uri_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let Some(uri) = mongo_uri(&ctx.cwd) else {
        return Err(RunError::Failed(
            "a mongodb URI was configured but is no longer resolvable".into(),
        ));
    };
    let Some((host, port)) = mongodb_target(&uri) else {
        emit("mongodb+srv:// URI configured (externally managed), nothing to start");
        return Ok(RunOutcome::Skipped(
            "external mongodb URI, nothing to start".into(),
        ));
    };
    if reachable(&host, port) {
        emit(&format!("mongodb responding on {host}:{port}"));
        Ok(RunOutcome::Skipped(format!(
            "mongodb already up ({host}:{port})"
        )))
    } else {
        emit(&format!("mongodb target {host}:{port} not reachable"));
        Ok(RunOutcome::Skipped(
            "mongodb URI configured, nothing to start".into(),
        ))
    }
}
