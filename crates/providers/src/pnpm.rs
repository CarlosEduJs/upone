//! pnpm provider: detects pnpm-lock.yaml and runs `pnpm install --frozen-lockfile`.

use upone_core::detect::Provider;

use crate::js::{add_install_plan, JsPm};

pub const PM: JsPm = JsPm {
    id: "pnpm",
    label: "pnpm",
    bin: "pnpm",
    signatures: &["pnpm-lock.yaml"],
    install_args: &["--frozen-lockfile"],
};

pub struct Pnpm;

impl Provider for Pnpm {
    fn id(&self) -> &'static str {
        "pnpm"
    }

    fn signatures(&self) -> &'static [&'static str] {
        PM.signatures
    }

    fn plan(&self, ctx: &upone_core::Context, planner: &mut upone_core::Planner<'_>) {
        add_install_plan(&PM, ctx, planner);
    }
}
