//! tRPC provider: recognizes tRPC via the `@trpc/server` dependency.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

use crate::cmd::package_has_dependency;

pub struct Trpc;

impl Provider for Trpc {
    fn id(&self) -> &'static str {
        "trpc"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        if package_has_dependency(cwd, "@trpc/server") {
            return Some(Detection {
                provider: self.id(),
                signature: "package.json (@trpc/server)".into(),
                reason: "tRPC API detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}