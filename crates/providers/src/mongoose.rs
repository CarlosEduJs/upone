//! Mongoose provider: recognizes the Mongoose `mongodb` ODM via the `mongoose`
//! dependency. Informational only — the `mongo` provider owns the database.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::Planner;
use upone_core::{Context, Detection};

use crate::cmd::package_has_dependency;

pub struct Mongoose;

impl Provider for Mongoose {
    fn id(&self) -> &'static str {
        "mongoose"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        if package_has_dependency(cwd, "mongoose") {
            return Some(Detection {
                provider: self.id(),
                signature: "package.json (mongoose)".into(),
                reason: "Mongoose (MongoDB ODM) detected".into(),
            });
        }
        None
    }

    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}
