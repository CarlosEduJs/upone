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

/// Collects all readiness checks from detected providers (evaluated in their
/// respective package contexts) and `.env.example` template keys across all
/// package directories.
pub fn collect_readiness_checks(
    root_ctx: &Context,
    package_detections: &[(&Context, &Detection)],
    registry: &Registry,
) -> Vec<ReadinessCheck> {
    let mut checks: Vec<ReadinessCheck> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // 1. Provider-inferred checks (evaluated per package context).
    for (pkg_ctx, d) in package_detections {
        if let Some(provider) = registry.all().iter().find(|p| p.id() == d.provider) {
            let rel = pkg_ctx
                .cwd
                .strip_prefix(&root_ctx.cwd)
                .ok()
                .filter(|r| !r.as_os_str().is_empty());
            let slug = rel.map(workspace::dir_slug);
            let rel_display = rel.map(|r| r.display().to_string());

            for mut check in provider.readiness_checks(pkg_ctx) {
                if let Some(s) = &slug {
                    check.id = format!("{s}-{}", check.id);
                    if let Some(r) = &rel_display {
                        check.label = format!("{} ({})", check.label, r);
                    }
                }
                if seen_ids.insert(check.id.clone()) {
                    checks.push(check);
                }
            }
        }
    }

    // 2. `.env.example` / `.env.template` keys per unique package directory.
    let mut unique_dirs: Vec<&Context> = vec![root_ctx];
    for (pkg_ctx, _) in package_detections {
        if !unique_dirs.iter().any(|c| c.cwd == pkg_ctx.cwd) {
            unique_dirs.push(pkg_ctx);
        }
    }

    for dir_ctx in unique_dirs {
        let rel = dir_ctx
            .cwd
            .strip_prefix(&root_ctx.cwd)
            .ok()
            .filter(|r| !r.as_os_str().is_empty());
        let slug = rel.map(workspace::dir_slug);
        let rel_display = rel.map(|r| r.display().to_string());

        let template_reqs = upone_core::env_requirements_from_template(&dir_ctx.cwd);
        for req in template_reqs {
            let (id, label) = match (&slug, &rel_display) {
                (Some(s), Some(r)) => (
                    format!("{s}-env-{}", req.key),
                    format!("{} ({})", req.key, r),
                ),
                _ => (format!("env-{}", req.key), req.key.clone()),
            };

            if seen_ids.contains(&id) {
                continue;
            }
            seen_ids.insert(id.clone());
            let key = req.key.clone();
            let importance = req.importance;
            let cwd = dir_ctx.cwd.clone();
            checks.push(ReadinessCheck::new(
                id,
                label,
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
    }

    checks
}
