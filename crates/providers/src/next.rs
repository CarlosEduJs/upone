//! Next.js provider: recognizes Next.js apps via package.json.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

use crate::cmd::package_has_dependency;

pub struct Next;

impl Provider for Next {
    fn id(&self) -> &'static str {
        "next"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["next.config.js", "next.config.mjs", "next.config.ts"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        for sig in self.signatures() {
            if cwd.join(sig).is_file() {
                return Some(Detection {
                    provider: self.id(),
                    signature: sig.to_string(),
                    reason: "next.js app detected".into(),
                });
            }
        }
        if package_has_dependency(cwd, "next") {
            return Some(Detection {
                provider: self.id(),
                signature: "package.json (next)".into(),
                reason: "next.js app detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}
