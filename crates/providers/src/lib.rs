//! Bundled technology providers.

pub mod better_auth;
pub mod biome;
pub mod bun;
pub mod cargo;
mod cmd;
pub mod docker;
pub mod drizzle;
mod js;
pub mod next;
pub mod npm;
pub mod pnpm;
pub mod postgres;
pub mod prisma;
pub mod redis;
pub mod shadcn;
pub mod trpc;
pub mod turbo;
pub mod workspace;

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
    reg.register(Box::new(turbo::Turbo));
    reg.register(Box::new(biome::Biome));
    reg.register(Box::new(shadcn::Shadcn));
    reg.register(Box::new(next::Next));
    reg.register(Box::new(trpc::Trpc));
    reg.register(Box::new(better_auth::BetterAuth));
    reg
}
