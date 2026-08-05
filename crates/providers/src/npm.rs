//! npm provider: detects package-lock.json and runs `npm install`.

use upone_core::detect::Provider;

use crate::js::{add_install_plan, JsPm};

pub const PM: JsPm = JsPm {
    id: "npm",
    label: "npm",
    bin: "npm",
    signatures: &["package-lock.json"],
    install_args: &["--no-audit", "--no-fund"],
};

pub struct Npm;

impl Provider for Npm {
    fn id(&self) -> &'static str {
        "npm"
    }

    fn signatures(&self) -> &'static [&'static str] {
        PM.signatures
    }

    fn plan(&self, ctx: &upone_core::Context, planner: &mut upone_core::Planner<'_>) {
        add_install_plan(&PM, ctx, planner);
    }
}
