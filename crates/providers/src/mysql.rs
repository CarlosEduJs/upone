//! `MySQL`/`MariaDB` provider: detects mysql or mariadb in docker-compose
//! (or `DATABASE_URL`) and ensures the service is up.
//!
//! `MariaDB` is a drop-in compatibility layer for `MySQL`, so a single
//! provider handles both. When a compose file defines the service, the `docker`
//! provider's `docker compose up -d` is the single owner that starts it; this
//! provider only depends on that task and verifies the server responds.
//! Without a compose definition, upone cannot start mysql for you, so it
//! reports a clear, actionable error instead of firing a broken command.

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

const ENV_FILES: &[&str] = &[".env", ".env.local"];

const LOCAL_PORT: u16 = 3306;

pub struct Mysql;

impl Provider for Mysql {
    fn id(&self) -> &'static str {
        "mysql"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["my.cnf", "mysql.conf"]
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
                    "checks that mysql responds on localhost:3306",
                )
                .risk(Risk::Low)
                .run(mysql_check),
            );
        }
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        use upone_core::readiness::{resolve_env_key, Importance, ReadinessCheck, ReadinessStatus};

        let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
        let mut checks = vec![ReadinessCheck::new(
            "mysql-tcp",
            format!("mysql/mariadb (localhost:{port})"),
            "MySQL is accepting TCP connections",
            Importance::Required,
            move |_ctx| {
                if mysql_reachable(port) {
                    ReadinessStatus::Ready(format!("responding on localhost:{port}"))
                } else {
                    ReadinessStatus::NotReady {
                        reason: format!("mysql not responding on localhost:{port}"),
                        remedy:
                            "Run 'docker compose up -d' or check if the mysql container is running"
                                .into(),
                    }
                }
            },
        )];

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

fn mysql_reachable(port: u16) -> bool {
    use std::net::TcpStream;
    let Ok(addr) = format!("127.0.0.1:{port}").parse() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// Compose-backed: the `docker-up` task already started the service; just confirm it responds.
fn mysql_verify(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
    if mysql_reachable(port) {
        emit(&format!("mysql responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("mysql already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "mysql not responding on localhost:{port} after the compose services started. Check `docker compose up -d` / `docker compose logs mysql`."
            ),
        ))
    }
}

/// No compose definition: nothing here can start mysql, so it reports clearly.
fn mysql_check(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let port = compose_host_port(&ctx.cwd, COMPOSE_FILES, LOCAL_PORT);
    if mysql_reachable(port) {
        emit(&format!("mysql responding on localhost:{port}"));
        Ok(RunOutcome::Skipped("mysql already up".into()))
    } else {
        Err(RunError::Failed(
            format!(
                "mysql is not responding on localhost:{port} and there is no docker-compose service to start it. \
                Start it yourself (e.g. `docker run -d -p {port}:3306 -e MYSQL_ROOT_PASSWORD=secret mysql`), then re-run upone."
            ),
        ))
    }
}
