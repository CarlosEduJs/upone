//! `poetry` provider: detects poetry.lock (in the absence of uv.lock) and
//! runs `poetry install`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, Task};
use upone_core::{Context, Detection, Risk};

use crate::cmd::spawn_cmd;

use super::python;

pub struct Poetry;

impl Provider for Poetry {
    fn id(&self) -> &'static str {
        "poetry"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["poetry.lock"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        // uv takes precedence over poetry when both lockfiles are present.
        if cwd.join("poetry.lock").is_file() && !cwd.join("uv.lock").is_file() {
            return Some(self.found("poetry.lock"));
        }
        None
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "poetry-check",
            "check poetry installed",
            "checks that poetry is on PATH",
        )
        .risk(Risk::Low)
        .run(|_ctx, emit| {
            python::check_binary(
                "poetry",
                "Install it via https://python-poetry.org/docs/#installation.",
                emit,
            )
        });

        let install = Task::new(
            "poetry-install",
            "poetry install",
            "installs the project and its dependencies with poetry (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["poetry-check"])
        .run(|ctx, emit| spawn_cmd("poetry", &["install"], &ctx.cwd, emit));

        planner.add(check);
        planner.add(install);
    }
}
