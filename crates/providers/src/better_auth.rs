//! Better Auth provider: recognizes `better-auth` via package.json.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

use crate::cmd::package_has_dependency;

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
        use upone_core::readiness::{resolve_env_key, Importance, ReadinessCheck, ReadinessStatus};

        let cwd = ctx.cwd.clone();
        vec![ReadinessCheck::new(
            "env-BETTER_AUTH_SECRET",
            "BETTER_AUTH_SECRET",
            "BETTER_AUTH_SECRET environment variable is set",
            Importance::Required,
            move |_ctx| {
                if resolve_env_key(&cwd, "BETTER_AUTH_SECRET").is_some() {
                    ReadinessStatus::Ready("found".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "BETTER_AUTH_SECRET not found in process env or .env* files".into(),
                        remedy: "Add BETTER_AUTH_SECRET to your .env.local or shell environment"
                            .into(),
                    }
                }
            },
        )]
    }
}
