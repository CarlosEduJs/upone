//! GORM provider: recognizes the `gorm.io/gorm` ORM via `go.mod`.
//!
//! Informational only — GORM runs migrations from Go code (`AutoMigrate`),
//! there is no universal migration CLI for upone to run. The `go` provider
//! owns installing/building the project.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

pub struct Gorm;

impl Provider for Gorm {
    fn id(&self) -> &'static str {
        "gorm"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        let content = std::fs::read_to_string(cwd.join("go.mod")).ok()?;
        let has_gorm = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("gorm.io/gorm") || trimmed.starts_with("github.com/jinzhu/gorm")
        });
        if has_gorm {
            return Some(Detection {
                provider: self.id(),
                signature: "go.mod (gorm)".into(),
                reason: "GORM ORM detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}
