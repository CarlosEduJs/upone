//! Bun provider: detects bun.lock/bun.lockb and runs `bun install`.

use upone_core::detect::Provider;

use crate::js::{add_install_plan, JsPm};

pub const PM: JsPm = JsPm {
    id: "bun",
    label: "bun",
    bin: "bun",
    signatures: &["bun.lock", "bun.lockb"],
    install_args: &[],
};

pub struct Bun;

impl Provider for Bun {
    fn id(&self) -> &'static str {
        "bun"
    }

    fn signatures(&self) -> &'static [&'static str] {
        PM.signatures
    }

    fn plan(&self, ctx: &upone_core::Context, planner: &mut upone_core::Planner<'_>) {
        add_install_plan(&PM, ctx, planner);
    }
}
