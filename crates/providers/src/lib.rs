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

use std::collections::HashSet;

use upone_core::detect::Registry;
use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};
use upone_core::{Context, Detection};

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

/// Collects all readiness checks from detected providers and `.env.example`
/// template keys. Provider-inferred checks come first, then any template
/// keys that weren't already covered by a provider check.
pub fn collect_readiness_checks(
    ctx: &Context,
    detections: &[Detection],
    registry: &Registry,
) -> Vec<ReadinessCheck> {
    let mut checks: Vec<ReadinessCheck> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // Provider-inferred checks.
    for d in detections {
        if let Some(provider) = registry.all().iter().find(|p| p.id() == d.provider) {
            for check in provider.readiness_checks(ctx) {
                if seen_ids.insert(check.id.clone()) {
                    checks.push(check);
                }
            }
        }
    }

    // `.env.example` / `.env.template` keys.
    let template_reqs = upone_core::env_requirements_from_template(&ctx.cwd);
    for req in template_reqs {
        let id = format!("env-{}", req.key);
        if seen_ids.contains(&id) {
            // Already covered by a provider check (e.g. DATABASE_URL from postgres).
            continue;
        }
        seen_ids.insert(id.clone());
        let key = req.key.clone();
        let importance = req.importance;
        let cwd = ctx.cwd.clone();
        checks.push(ReadinessCheck::new(
            id,
            key.clone(),
            format!("{} environment variable", key),
            importance,
            move |_ctx| {
                if upone_core::resolve_env_key(&cwd, &key).is_some() {
                    ReadinessStatus::Ready("found".into())
                } else {
                    let remedy = format!("Add {} to your .env.local or shell environment", key);
                    if importance == Importance::Optional {
                        ReadinessStatus::Warning {
                            reason: format!("{} not found (optional)", key),
                            remedy,
                        }
                    } else {
                        ReadinessStatus::NotReady {
                            reason: format!("{} not found in process env or .env* files", key),
                            remedy,
                        }
                    }
                }
            },
        ));
    }

    checks
}
