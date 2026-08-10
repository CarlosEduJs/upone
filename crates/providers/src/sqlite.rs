//! `SQLite` provider: detects a sqlite-backed `DATABASE_URL` (or an ORM
//! config that targets sqlite) and ensures the database file exists.
//!
//! Unlike postgres/redis/mysql/mongo there is no server to start: sqlite is an
//! embedded engine. The only "prepare" a sqlite project needs is a database
//! file that exists and is writable, so upone creates an empty one when the
//! resolved `DATABASE_URL` — or the detected Prisma/Drizzle/Alembic config —
//! points at a missing path (safe and idempotent). When no file path can be
//! resolved at all, the provider detects the project but adds no task.

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

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        // Nothing to ensure when the file path cannot be resolved from the
        // environment or the detected ORM config.
        if sqlite_path(&ctx.cwd).is_none() {
            return;
        }
        planner.add(
            Task::new(
                "sqlite-ensure",
                "ensure sqlite database file",
                "creates the sqlite database file from DATABASE_URL or the detected ORM config if it does not exist (safe to repeat)",
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
            "the sqlite database file resolved from DATABASE_URL or the ORM config exists",
            Importance::Required,
            move |_ctx| {
                match sqlite_path(&cwd) {
                Some(path) if path.is_file() => {
                    ReadinessStatus::Ready(format!("present at {}", path.display()))
                }
                Some(path) => ReadinessStatus::NotReady {
                    reason: format!("sqlite database file missing at {}", path.display()),
                    remedy: "Run 'upone up' to create it, or create the file yourself".into(),
                },
                None => ReadinessStatus::NotReady {
                    reason: "could not resolve a sqlite database file from DATABASE_URL or the detected ORM config".into(),
                    remedy: "Set a sqlite DATABASE_URL (or point the ORM config at a file) in your .env.local or shell environment".into(),
                },
            }
            },
        )]
    }
}

/// Resolves the sqlite database file path: first from the `DATABASE_URL`
/// environment key, then from the detected Prisma/Drizzle/Alembic config.
fn sqlite_path(cwd: &Path) -> Option<PathBuf> {
    if let Some(url) = resolve_env_key(cwd, "DATABASE_URL") {
        if let Some(path) = sqlite_path_from_url(cwd, &url) {
            return Some(path);
        }
    }
    sqlite_urls_from_configs(cwd)
        .iter()
        .find_map(|url| sqlite_path_from_url(cwd, url))
}

/// Resolves a sqlite database file path from a connection URL.
///
/// Supports the URL forms sqlite clients accept, with an optional query string
/// (`?mode=ro`, `?_pragma=...`) that is stripped:
///
/// - `sqlite:///abs/path` → absolute (`/abs/path`)
/// - `sqlite:///./rel.db` / `sqlite:///../rel.db` → relative to the project
/// - `sqlite://rel.db` / `sqlite:rel.db` → relative to the project
/// - `file:path` → passed through as-is
fn sqlite_path_from_url(cwd: &Path, url: &str) -> Option<PathBuf> {
    let url = match url.split_once('?') {
        Some((path, _)) => path,
        None => url,
    };
    let rest = if let Some(r) = url.strip_prefix("sqlite://") {
        // A leading `/` marks an absolute filesystem path (`sqlite:///tmp/x.db`),
        // except for the `/./`/`/../` forms, which stay project-relative
        // (`sqlite:///./dev.db`).
        if r.starts_with("/./") || r.starts_with("/../") {
            &r[1..]
        } else {
            r
        }
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

/// Collects sqlite connection URLs declared in the detected ORM configs:
/// Prisma (`url = "sqlite:..."` in the datasource), Alembic
/// (`sqlalchemy.url = sqlite:///...`) and Drizzle (`url: "sqlite:..."`).
fn sqlite_urls_from_configs(cwd: &Path) -> Vec<String> {
    let mut urls = Vec::new();

    if let Ok(content) = std::fs::read_to_string(cwd.join("prisma").join("schema.prisma")) {
        for line in content.lines() {
            if line.trim().starts_with("url") {
                if let Some(val) = extract_quoted(line) {
                    if is_sqlite_url(val) {
                        urls.push(val.to_string());
                    }
                }
            }
        }
    }

    if let Ok(content) = std::fs::read_to_string(cwd.join("alembic.ini")) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.to_lowercase().starts_with("sqlalchemy.url") {
                let val = trimmed.split_once('=').map_or("", |(_, v)| v.trim());
                if is_sqlite_url(val) {
                    urls.push(val.to_string());
                }
            }
        }
    }

    for cfg in DRIZZLE_CONFIGS {
        if let Ok(content) = std::fs::read_to_string(cwd.join(cfg)) {
            for val in find_quoted_sqlite_urls(&content) {
                urls.push(val);
            }
        }
    }

    urls
}

/// Extracts the quoted value of an `key = "value"` line.
fn extract_quoted(line: &str) -> Option<&str> {
    let after = line.split_once('=')?.1.trim();
    after
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| after.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
}

/// True when `value` looks like a sqlite connection URL.
fn is_sqlite_url(value: &str) -> bool {
    value.starts_with("sqlite://") || value.starts_with("sqlite:") || value.starts_with("file:")
}

/// Finds quoted strings that are sqlite connection URLs in a config file
/// (used for Drizzle configs, which are TS/JS and not reliably parseable).
fn find_quoted_sqlite_urls(content: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for quote in ['"', '\''] {
        let mut rest = content;
        while let Some(start) = rest.find(quote) {
            rest = &rest[start + 1..];
            let Some(end) = rest.find(quote) else {
                break;
            };
            let candidate = rest[..end].trim();
            if is_sqlite_url(candidate) {
                urls.push(candidate.to_string());
            }
            rest = &rest[end + 1..];
        }
    }
    urls
}

/// Returns a copy of `path` with `.` components removed and `..` components
/// resolved lexically (e.g. `/a/./b/../c` → `/a/c`).
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Pop the last normal component if the parent dir refers to
                // one; otherwise keep the `..` (e.g. when it would go above a
                // relative prefix or the filesystem root).
                let pop = out
                    .components()
                    .next_back()
                    .is_some_and(|lc| matches!(lc, Component::Normal(_)));
                if pop {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
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
        crate::testkit::temp_dir("sqlite", name)
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
        // `sqlite:///abs` keeps its leading slash and resolves absolute...
        let dir = temp_dir("abs");
        with_env(&dir, "sqlite:///tmp/upone-abs-test.db");
        assert_eq!(
            sqlite_path(&dir),
            Some(PathBuf::from("/tmp/upone-abs-test.db"))
        );
        let _ = std::fs::remove_dir_all(&dir);

        // ...as does the four-slash form.
        let dir = temp_dir("abs4");
        with_env(&dir, "sqlite:////tmp/upone-abs-test.db");
        assert_eq!(
            sqlite_path(&dir),
            Some(PathBuf::from("/tmp/upone-abs-test.db"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_parent_relative() {
        let dir = temp_dir("rel-dotdot");
        with_env(&dir, "sqlite:///../upone-abs-test.db");
        let expected = dir.parent().unwrap().join("upone-abs-test.db");
        assert_eq!(sqlite_path(&dir), Some(expected));
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

    #[test]
    fn resolves_from_prisma_config() {
        let dir = temp_dir("prisma-cfg");
        std::fs::create_dir_all(dir.join("prisma")).unwrap();
        std::fs::write(
            dir.join("prisma").join("schema.prisma"),
            "datasource db {\n  provider = \"sqlite\"\n  url = \"sqlite:data/app.db\"\n}\n",
        )
        .unwrap();
        assert_eq!(sqlite_path(&dir), Some(dir.join("data/app.db")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_from_drizzle_config() {
        let dir = temp_dir("drizzle-cfg");
        std::fs::write(
            dir.join("drizzle.config.ts"),
            "export default { dbCredentials: { url: \"sqlite:data/app.db\" } }",
        )
        .unwrap();
        assert_eq!(sqlite_path(&dir), Some(dir.join("data/app.db")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_from_alembic_config() {
        let dir = temp_dir("alembic-cfg");
        std::fs::write(
            dir.join("alembic.ini"),
            "sqlalchemy.url = sqlite:///./server/dev.db\n",
        )
        .unwrap();
        assert_eq!(sqlite_path(&dir), Some(dir.join("server/dev.db")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_sqlite_config_urls_are_ignored() {
        let dir = temp_dir("cfg-other");
        std::fs::create_dir_all(dir.join("prisma")).unwrap();
        std::fs::write(
            dir.join("prisma").join("schema.prisma"),
            "datasource db {\n  provider = \"postgresql\"\n  url = \"postgres://localhost/app\"\n}\n",
        )
        .unwrap();
        assert_eq!(sqlite_path(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
