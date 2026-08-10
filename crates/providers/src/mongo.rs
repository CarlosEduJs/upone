//! `MongoDB` provider: detects mongodb in docker-compose or a `mongodb://` /
//! `mongodb+srv://` URI and verifies the service is up.
//!
//! When a compose file defines the service, the `docker` provider's
//! `docker compose up -d` is the single owner that starts it; this provider
//! depends on that task and verifies the server responds. Without a compose
//! definition upone cannot start mongodb, so an externally configured URI is
//! only verified against its own target — it is never started.

use std::net::ToSocketAddrs;
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

const MONGODB_KEYS: &[&str] = &["MONGODB_URI", "MONGO_URI", "DATABASE_URL"];

const ENV_FILES: &[&str] = &[".env", ".env.local"];

const LOCAL_PORT: u16 = 27017;

pub struct Mongo;

impl Provider for Mongo {
    fn id(&self) -> &'static str {
        "mongo"
    }

    fn signatures(&self) -> &'static [&'static str] {
        // Detected by content (docker-compose/env URI), not by file signatures.
        &[]
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
if tcp_reachable("127.0.0.1", port) {
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
                    Some((host, port)) if tcp_reachable(&host, port) => {
                        ReadinessStatus::Ready(format!("responding on {host}:{port}"))
                    }
                    Some((host, port)) => ReadinessStatus::NotReady {
                        reason: format!("mongodb target {host}:{port} not responding"),
                        remedy: "Check the mongodb service for the configured URI".into(),
                    },
                    // `mongodb+srv://` has no single target to probe — the
                    // authoritative check is that the hostname resolves at all,
                    // so a typo'd Atlas host fails here instead of passing green.
                    None => srv_hostname(&uri).map_or_else(
                        || ReadinessStatus::NotReady {
                            reason: "could not parse a hostname from the mongodb URI".into(),
                            remedy: "Check the MONGODB_URI / MONGO_URI / DATABASE_URL value".into(),
                        },
                        |host| {
                            if hostname_resolves(&host) {
                                ReadinessStatus::Ready(
                                    "external mongodb+srv target resolves (no local server to check)".into(),
                                )
                            } else {
                                ReadinessStatus::NotReady {
                                    reason: format!("mongodb+srv hostname {host} does not resolve"),
                                    remedy: "Check the mongodb+srv connection string".into(),
                                }
                            }
                        },
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
    parse_uri_authority(uri, &["mongodb"], LOCAL_PORT)
}

/// Extracts the hostname of a `mongodb+srv://` URI (the scheme these URIs use
/// instead of a fixed `host:port`), stripping credentials. Returns `None` when
/// the URI is not an srv URI or has no hostname.
fn srv_hostname(uri: &str) -> Option<String> {
    let authority = uri.strip_prefix("mongodb+srv://")?.split('/').next()?;
    // A query string may follow the authority without a path separator
    // (`host/?authSource=admin`); strip it before reading the host.
    let authority = authority.split_once('?').map_or(authority, |(a, _)| a);
    let authority = authority.rsplit('@').next()?;
    let host = authority.split_once(':').map_or(authority, |(h, _)| h);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// True when the hostname resolves to at least one address (DNS lookup, no
/// connection). Used as the readiness probe for externally-managed `srv` URIs.
/// The lookup runs on a worker thread with a bounded wait, so a slow or
/// unresponsive resolver cannot hang the readiness sweep indefinitely.
fn hostname_resolves(host: &str) -> bool {
    use std::sync::mpsc;
    use std::time::Duration;

    let host = host.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let ok = format!("{host}:27017")
            .to_socket_addrs()
            .is_ok_and(|mut addrs| addrs.next().is_some());
        let _ = tx.send(ok);
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap_or(false)
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn mongo_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
    if tcp_reachable("127.0.0.1", port) {
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
    if tcp_reachable(&host, port) {
        emit(&format!("mongodb responding on {host}:{port}"));
        Ok(RunOutcome::Skipped(format!(
            "mongodb already up ({host}:{port})"
        )))
    } else {
        emit(&format!("mongodb target {host}:{port} not reachable"));
        Err(RunError::Failed(format!(
            "mongodb is not responding on {host}:{port} and there is no docker-compose service to start it. \
            Start it yourself (e.g. `docker run -d -p {port}:27017 mongo`), then re-run upone."
        )))
    }
}
