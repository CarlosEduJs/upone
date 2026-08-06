//! Biome provider: recognizes Biome via `biome.json` / `biome.jsonc`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

pub struct Biome;

impl Provider for Biome {
    fn id(&self) -> &'static str {
        "biome"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["biome.json", "biome.jsonc"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        for sig in self.signatures() {
            if cwd.join(sig).is_file() {
                return Some(Detection {
                    provider: self.id(),
                    signature: (*sig).to_string(),
                    reason: "biome detected".into(),
                });
            }
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}