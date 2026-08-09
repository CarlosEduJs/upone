//! Better Auth provider: recognizes `better-auth` via package.json.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

use crate::cmd::{env_key_check, package_has_dependency};

pub struct BetterAuth;

impl Provider for BetterAuth {
    fn id(&self) -> &'static str {
        "better-auth"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        if package_has_dependency(cwd, "better-auth") {
            return Some(Detection {
                provider: self.id(),
                signature: "package.json (better-auth)".into(),
                reason: "better-auth detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}

    fn readiness_checks(&self, ctx: &Context) -> Vec<upone_core::readiness::ReadinessCheck> {
        vec![env_key_check(
            "env-BETTER_AUTH_SECRET",
            "BETTER_AUTH_SECRET",
            &ctx.cwd,
        )]
    }
}
