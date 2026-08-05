//! Bundled technology providers.

pub mod bun;
pub mod cargo;
mod cmd;
pub mod docker;
pub mod drizzle;
mod js;
pub mod npm;
pub mod pnpm;
pub mod postgres;
pub mod prisma;
pub mod redis;

use upone_core::detect::Registry;

/// Registers all bundled providers.
pub fn build_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Box::new(bun::Bun));
    reg.register(Box::new(cargo::Cargo));
    reg.register(Box::new(npm::Npm));
    reg.register(Box::new(pnpm::Pnpm));
    reg.register(Box::new(docker::Docker));
    reg.register(Box::new(prisma::Prisma));
    reg.register(Box::new(drizzle::Drizzle));
    reg.register(Box::new(redis::Redis));
    reg.register(Box::new(postgres::Postgres));
    reg
}
