//! `MySQL`/`MariaDB` provider: detects mysql or mariadb in docker-compose
//! (or `DATABASE_URL`) and ensures the service is up.
//!
//! `MariaDB` is a drop-in compatibility layer for `MySQL`, so a single
//! provider handles both. When a compose file defines the service, the `docker`
//! provider's `docker compose up -d` is the single owner that starts it; this
//! provider depends on that task and verifies the server responds. Without a
//! compose definition, the configured `DATABASE_URL` authority is checked
//! directly — an external authority (e.g. `db.internal:3307`) is validated, not
//! started.

use std::path::Path;
use std::time::Duration;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::resolve_env_key;
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{
    compose_host_port, env_key_check, files_contain, parse_uri_authority, tcp_reachable,
};

const COMPOSE_FILES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

const ENV_FILES: &[&str] = &[".env", ".env.local"];

const LOCAL_PORT: u16 = 3306;

/// Bounded wait for the compose service to start responding.
const VERIFY_DEADLINE: Duration = Duration::from_secs(30);

pub struct Mysql;

impl Provider for Mysql {
    fn id(&self) -> &'static str {
        "mysql"
    }

    fn signatures(&self) -> &'static [&'static str] {
        // Detected by content (docker-compose/DATABASE_URL), not by file signatures.
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if files_contain(cwd, COMPOSE_FILES, &["mysql", "mariadb"]) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "docker-compose (mysql/mariadb service)".into(),
                reason: "mysql detected in docker-compose".into(),
            });
        }
        if files_contain(
            cwd,
            ENV_FILES,
            &["DATABASE_URL=mysql", "mysql://", "mariadb://"],
        ) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: ".env (DATABASE_URL mysql)".into(),
                reason: "mysql detected via DATABASE_URL".into(),
            });
        }
        None
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        if files_contain(&ctx.cwd, COMPOSE_FILES, &["mysql", "mariadb"]) {
            planner.add(
                Task::new(
                    "mysql-up",
                    "verify mysql is running",
                    "checks that mysql responds after the compose service starts",
                )
                .risk(Risk::Low)
                .depends_on(["docker-up"])
                .run(mysql_verify),
            );
        } else {
            planner.add(
                Task::new(
                    "mysql-up",
                    "check mysql is running",
                    "checks that mysql responds on its configured target",
                )
                .risk(Risk::Low)
                .run(mysql_check),
            );
        }
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};

        // Compose-backed flows keep the localhost behavior; otherwise the
        // configured DATABASE_URL authority is checked directly.
        let (host, port) = mysql_target(&ctx.cwd);
        let mut checks = vec![ReadinessCheck::new(
            "mysql-tcp",
            format!("mysql/mariadb ({host}:{port})"),
            "MySQL is accepting TCP connections",
            Importance::Required,
            move |_| {
                if tcp_reachable(&host, port) {
                    ReadinessStatus::Ready(format!("responding on {host}:{port}"))
                } else {
                    ReadinessStatus::NotReady {
                        reason: format!("mysql not responding on {host}:{port}"),
                        remedy:
                            "Run 'docker compose up -d' or check if the mysql container is running"
                                .into(),
                    }
                }
            },
        )];

        checks.push(env_key_check("env-DATABASE_URL", "DATABASE_URL", &ctx.cwd));

        checks
    }
}

/// Resolves the mysql/mariadb target to check: `127.0.0.1` on the compose host
/// port for Compose-backed services, the `DATABASE_URL` authority otherwise.
fn mysql_target(cwd: &Path) -> (String, u16) {
    if files_contain(cwd, COMPOSE_FILES, &["mysql", "mariadb"]) {
        (
            "127.0.0.1".into(),
            compose_host_port(cwd, COMPOSE_FILES, LOCAL_PORT),
        )
    } else if let Some(target) = mysql_database_url_target(cwd) {
        target
    } else {
        ("127.0.0.1".into(), LOCAL_PORT)
    }
}

/// Parses the `host[:port]` authority of a mysql/mariadb `DATABASE_URL`,
/// defaulting the port to 3306 when absent. Returns `None` when the value is
/// not a mysql/mariadb URL or has no resolvable authority (e.g. unix sockets).
fn mysql_database_url_target(cwd: &Path) -> Option<(String, u16)> {
    let url = resolve_env_key(cwd, "DATABASE_URL")?;
    parse_uri_authority(&url, &["mysql", "mariadb"], LOCAL_PORT)
}

/// Compose-backed: the `docker-up` task started the service; poll until it
/// responds (compose containers can take a few seconds to accept connections).
fn mysql_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let (host, port) = mysql_target(&ctx.cwd);
    let deadline = std::time::Instant::now() + VERIFY_DEADLINE;
    loop {
        if tcp_reachable(&host, port) {
            emit(&format!("mysql responding on {host}:{port}"));
            return Ok(RunOutcome::Skipped("mysql already up".into()));
        }
        if std::time::Instant::now() >= deadline {
            return Err(RunError::Failed(format!(
                "mysql not responding on {host}:{port} after the compose services started. Check `docker compose up -d` / `docker compose logs mysql`."
            )));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// No compose definition: nothing here can start mysql, so it reports clearly.
fn mysql_check(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let (host, port) = mysql_target(&ctx.cwd);
    if tcp_reachable(&host, port) {
        emit(&format!("mysql responding on {host}:{port}"));
        Ok(RunOutcome::Skipped("mysql already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "mysql is not responding on {host}:{port} and there is no docker-compose service to start it. \
                Start it yourself (e.g. `docker run -d -p {port}:3306 -e MYSQL_ROOT_PASSWORD=secret mysql`), then re-run upone."
            ),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        crate::testkit::temp_dir("mysql", name)
    }

    fn with_env(dir: &Path, url: &str) {
        std::fs::write(dir.join(".env"), format!("DATABASE_URL={url}\n")).unwrap();
    }

    #[test]
    fn resolves_external_authority() {
        let dir = temp_dir("ext");
        with_env(&dir, "mysql://user:pass@db.internal:3307/app");
        assert_eq!(
            mysql_database_url_target(&dir),
            Some(("db.internal".into(), 3307))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn defaults_port_when_absent() {
        let dir = temp_dir("noport");
        with_env(&dir, "mysql://user:pass@db.internal/app");
        assert_eq!(
            mysql_database_url_target(&dir),
            Some(("db.internal".into(), 3306))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_other_schemes() {
        let dir = temp_dir("other");
        with_env(&dir, "postgres://localhost/db");
        assert_eq!(mysql_database_url_target(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
