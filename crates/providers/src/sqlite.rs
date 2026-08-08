//! `SQLite` provider: detects a sqlite-backed `DATABASE_URL` (or an ORM
//! config that targets sqlite) and ensures the database file exists.
//!
//! Unlike postgres/redis/mysql/mongo there is no server to start: sqlite is an
//! embedded engine. The only "prepare" a sqlite project needs is a database
//! file that exists and is writable, so upone creates an empty one when the
//! resolved `DATABASE_URL` points at a missing path (safe and idempotent).

use std::path::{Path, PathBuf};

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::{resolve_env_key, Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::files_contain;

const ENV_FILES: &[&str] = &[".env.development", ".env.local", ".env"];

const DRIZZLE_CONFIGS: &[&str] = &[
    "drizzle.config.ts",
    "drizzle.config.js",
    "drizzle.config.json",
    "drizzle.config.mts",
];

pub struct Sqlite;

impl Provider for Sqlite {
    fn id(&self) -> &'static str {
        "sqlite"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        if files_contain(cwd, ENV_FILES, &["sqlite://", "DATABASE_URL=sqlite"]) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: ".env (DATABASE_URL sqlite)".into(),
                reason: "sqlite detected via DATABASE_URL".into(),
            });
        }
        if files_contain(cwd, &["prisma/schema.prisma"], &["provider = \"sqlite\""]) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "prisma/schema.prisma (sqlite)".into(),
                reason: "sqlite detected via Prisma schema".into(),
            });
        }
        if files_contain(cwd, DRIZZLE_CONFIGS, &["sqlite"]) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "drizzle.config.* (sqlite)".into(),
                reason: "sqlite detected via drizzle config".into(),
            });
        }
        if files_contain(cwd, &["alembic.ini"], &["sqlite://"]) {
            return Some(upone_core::Detection {
                provider: self.id(),
                signature: "alembic.ini (sqlite)".into(),
                reason: "sqlite detected via alembic config".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        planner.add(
            Task::new(
                "sqlite-ensure",
                "ensure sqlite database file",
                "creates the sqlite database file from DATABASE_URL if it does not exist (safe to repeat)",
            )
            .risk(Risk::Low)
            .run(sqlite_ensure),
        );
    }

    fn readiness_checks(&self, ctx: &Context) -> Vec<ReadinessCheck> {
        let cwd = ctx.cwd.clone();
        vec![ReadinessCheck::new(
            "sqlite-file",
            "sqlite database file",
            "the sqlite database file from DATABASE_URL exists",
            Importance::Required,
            move |_ctx| match sqlite_path(&cwd) {
                Some(path) if path.is_file() => {
                    ReadinessStatus::Ready(format!("present at {}", path.display()))
                }
                Some(path) => ReadinessStatus::NotReady {
                    reason: format!("sqlite database file missing at {}", path.display()),
                    remedy: "Run 'upone up' to create it, or create the file yourself".into(),
                },
                None => ReadinessStatus::NotReady {
                    reason: "DATABASE_URL (sqlite://) not found in process env or .env* files"
                        .into(),
                    remedy: "Add a sqlite DATABASE_URL to your .env.local or shell environment"
                        .into(),
                },
            },
        )]
    }
}

/// Resolves the sqlite database file path declared in `DATABASE_URL`.
///
/// Supports the URL forms sqlite clients accept, with an optional query string
/// (`?mode=ro`, `?_pragma=...`) that is stripped:
///
/// - `sqlite:///abs/path` → absolute (extra leading `/` marks the URL path)
/// - `sqlite:////abs/path` → absolute
/// - `sqlite://./rel.db` / `sqlite://rel.db` → relative to the project
/// - `sqlite:path`, `file:path` → passed through as-is
fn sqlite_path(cwd: &Path) -> Option<PathBuf> {
    let url = resolve_env_key(cwd, "DATABASE_URL")?;
    let url = match url.split_once('?') {
        Some((path, _)) => path.to_string(),
        None => url,
    };
    let rest = if let Some(r) = url.strip_prefix("sqlite://") {
        // The first `/` after the scheme is a URL separator; `//abs` is absolute.
        r.strip_prefix('/').unwrap_or(r)
    } else if let Some(r) = url.strip_prefix("sqlite:") {
        r
    } else {
        url.strip_prefix("file:")?
    };
    if rest.is_empty() {
        return None;
    }
    // Joining onto cwd yields an absolute path when `rest` is absolute;
    // normalize away any embedded `.` components left by relative URLs.
    Some(normalize_path(&cwd.join(rest)))
}

/// Returns a copy of `path` with `.` components removed (e.g. `/a/./b` → `/a/b`).
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        if !matches!(c, Component::CurDir) {
            out.push(c.as_os_str());
        }
    }
    out
}

fn sqlite_ensure(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    let Some(path) = sqlite_path(&ctx.cwd) else {
        return Err(RunError::Failed(
            "sqlite detected but no sqlite:// DATABASE_URL to resolve a file from. \
             Add DATABASE_URL=sqlite:///path/to/app.db to your .env"
                .into(),
        ));
    };

    if path.is_file() {
        emit(&format!("sqlite database present at {}", path.display()));
        return Ok(RunOutcome::Skipped("sqlite database exists".into()));
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RunError::Failed(format!(
                    "could not create {} for the sqlite database: {e}",
                    parent.display()
                ))
            })?;
        }
    }

    std::fs::write(&path, []).map_err(|e| {
        RunError::Failed(format!(
            "could not create the sqlite database file at {}: {e}",
            path.display()
        ))
    })?;
    emit(&format!(
        "created sqlite database file at {}",
        path.display()
    ));
    Ok(RunOutcome::Ran("created".into()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("upone-sqlite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn with_env(dir: &Path, value: &str) {
        std::fs::write(dir.join(".env"), format!("DATABASE_URL={value}\n")).unwrap();
    }

    #[test]
    fn resolves_relative_path_from_leading_dot() {
        let dir = temp_dir("rel-dot");
        with_env(&dir, "sqlite:///./app.db");
        assert_eq!(sqlite_path(&dir), Some(dir.join("app.db")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_plain_relative_path() {
        let dir = temp_dir("rel-plain");
        with_env(&dir, "sqlite://dev.db");
        assert_eq!(sqlite_path(&dir), Some(dir.join("dev.db")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_absolute_path() {
        let dir = temp_dir("abs");
        with_env(&dir, "sqlite:////tmp/upone-abs-test.db");
        assert_eq!(
            sqlite_path(&dir),
            Some(PathBuf::from("/tmp/upone-abs-test.db"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_file_scheme() {
        let dir = temp_dir("file");
        with_env(&dir, "file:data.db");
        assert_eq!(sqlite_path(&dir), Some(dir.join("data.db")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strips_query_string() {
        let dir = temp_dir("query");
        with_env(&dir, "sqlite:///./app.db?mode=ro");
        assert_eq!(sqlite_path(&dir), Some(dir.join("app.db")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_creates_missing_file() {
        let dir = temp_dir("ensure");
        with_env(&dir, "sqlite:///./nested/app.db");
        let path = sqlite_path(&dir).unwrap();
        let mut emit = |_: &str| {};
        let outcome = sqlite_ensure(&Context { cwd: dir.clone() }, &mut emit).unwrap();
        assert!(matches!(outcome, RunOutcome::Ran(_)));
        assert!(path.is_file());
        // Second run skips.
        let outcome = sqlite_ensure(&Context { cwd: dir.clone() }, &mut emit).unwrap();
        assert!(matches!(outcome, RunOutcome::Skipped(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
