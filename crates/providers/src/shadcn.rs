//! shadcn/ui provider: recognizes shadcn via `components.json`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

pub struct Shadcn;

impl Provider for Shadcn {
    fn id(&self) -> &'static str {
        "shadcn"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["components.json"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        if cwd.join("components.json").is_file() {
            return Some(Detection {
                provider: self.id(),
                signature: "components.json".into(),
                reason: "shadcn/ui components detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}