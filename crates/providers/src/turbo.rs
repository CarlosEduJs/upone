//! Turbo provider: recognizes Turborepo workspaces.
//!
//! Informational detection only — orchestration and build/lint tasks are out
//! of scope for `upone`, which prepares the environment (install, generate,
//! start services) rather than running builds.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

pub struct Turbo;

impl Provider for Turbo {
    fn id(&self) -> &'static str {
        "turbo"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["turbo.json"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        if cwd.join("turbo.json").is_file() {
            return Some(Detection {
                provider: self.id(),
                signature: "turbo.json".into(),
                reason: "turborepo workspace detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}
