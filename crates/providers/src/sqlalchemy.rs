//! `SQLAlchemy` provider: recognizes the `sqlalchemy` ORM via requirements
//! manifests or `pyproject.toml`.
//!
//! Informational only — `SQLAlchemy` migrations are delegated to `Alembic` (its
//! own `alembic` provider runs them when `alembic.ini` is present), and upone
//! has no universal `SQLAlchemy` CLI to invoke.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

use crate::cmd::files_contain;

pub struct SqlAlchemy;

impl Provider for SqlAlchemy {
    fn id(&self) -> &'static str {
        "sqlalchemy"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        let requirements = crate::python::requirements_file(cwd).map_or_else(
            || "requirements*.txt".into(),
            |p| {
                p.strip_prefix(cwd).map_or_else(
                    |_| p.to_string_lossy().into_owned(),
                    |rel| rel.to_string_lossy().into_owned(),
                )
            },
        );
        let pyproject = cwd.join("pyproject.toml");
        if files_contain(cwd, &[requirements.as_str()], &["sqlalchemy"])
            || contains_sqlalchemy(&pyproject)
        {
            return Some(Detection {
                provider: self.id(),
                signature: "python manifest (sqlalchemy)".into(),
                reason: "SQLAlchemy ORM detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}

fn contains_sqlalchemy(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|content| {
        // Match a dependency line, not a stray comment or project name.
        content.to_lowercase().contains("sqlalchemy")
    })
}
