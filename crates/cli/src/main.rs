//! `upone` — prepares development environments with a single command.

#![allow(clippy::print_stdout)]

mod report;
mod tui;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::sync::{mpsc, Arc};

use clap::Parser;
use upone_core::{Context, Detected, Detection, Engine, Event, Report, Task};
use upone_providers::{build_registry, collect_readiness_checks, workspace};

#[derive(Debug, clap::Parser)]
#[command(name = "upone", version, about = "prepares development environments")]
#[command(disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Detects the project and gets it ready for development.
    Up {
        /// Only shows the plan, runs nothing.
        #[arg(long)]
        dry_run: bool,
        /// Runs without asking for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Checks whether the development environment is ready.
    Ready,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let ctx = Context { cwd };
    let registry = build_registry();

    match cli.command {
        Command::Up { dry_run, yes } => cmd_up(&ctx, &registry, dry_run, yes),
        Command::Ready => {
            cmd_ready(&ctx, &registry);
            Ok(())
        }
    }
}

// ── upone up ────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
fn cmd_up(
    ctx: &Context,
    registry: &upone_core::Registry,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<()> {
    // Monorepos: detect at the root and at every workspace package, so a
    // project where drizzle lives under `packages/db` is still recognized.
    let root = ctx.cwd.clone();
    let mut all_dirs = vec![root.clone()];
    all_dirs.extend(workspace::package_dirs(&root));

    let mut detections = Detected::default();
    let mut pkg_detections: Vec<(Context, Detection)> = Vec::new();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let mut planner = upone_core::Planner::new(ctx);
    for dir in &all_dirs {
        let rel = dir
            .strip_prefix(&root)
            .ok()
            .filter(|rel| !rel.as_os_str().is_empty());
        let slug = rel.map(workspace::dir_slug);
        let rel_display = rel.map(|r| r.display().to_string());
        let dir_ctx = Context { cwd: dir.clone() };

        // Plan this directory's providers with its own cwd so tasks built
        // here know where to run (e.g. `drizzle-kit generate` in packages/db).
        let dir_detections = upone_core::detect::detect(dir, registry);
        let mut sub_planner = upone_core::Planner::new(&dir_ctx);
        for d in &dir_detections.found {
            if let Some(provider) = registry.all().iter().find(|p| p.id() == d.provider) {
                provider.plan(&dir_ctx, &mut sub_planner);
            }
        }
        // Relaxed: a package may depend on the root install task (bun-install),
        // which is validated once all plans are merged below.
        let local_plan = sub_planner
            .build_allow_external()
            .map_err(|e| anyhow::anyhow!("failed to build the plan: {e}"))?;

        // Surface detections with their package location in the reason.
        for d in &dir_detections.found {
            // Distinct packages may report the same provider+signature (e.g.
            // two packages with drizzle); keep them separate. Within one
            // package a provider matches at most once, so no further dedup.
            let key = (
                rel_display.clone().unwrap_or_default(),
                d.provider.to_string(),
                d.signature.clone(),
            );
            if !seen.insert(key) {
                continue;
            }
            pkg_detections.push((dir_ctx.clone(), d.clone()));
            let d = d.clone();
            let reason = match &rel_display {
                Some(r) => format!("{0} ({r})", d.reason),
                None => d.reason,
            };
            detections.found.push(Detection {
                provider: d.provider,
                signature: d.signature,
                reason,
            });
        }

        // Namespace per-package task ids so the same tech in two packages
        // doesn't collide. Root tasks (install etc.) keep their canonical ids.
        let local_ids: HashSet<String> = local_plan.ids().into_iter().collect();
        for id in local_plan.ids() {
            let Some(task) = local_plan.task(&id).cloned() else {
                continue;
            };
            let (new_id, new_deps) = match &slug {
                None => (id, task.deps),
                Some(s) => (
                    format!("{s}-{id}"),
                    task.deps
                        .into_iter()
                        .map(|d| {
                            if local_ids.contains(&d) {
                                format!("{s}-{d}")
                            } else {
                                d
                            }
                        })
                        .collect(),
                ),
            };
            planner.add(Task {
                id: new_id,
                deps: new_deps,
                ..task
            });
        }
    }

    let plan = planner
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build the plan: {e}"))?;

    if detections.is_empty() {
        report::no_project(ctx);
        return Ok(());
    }

    report::preview(&detections, &plan);

    if dry_run {
        report::dry_run_done();
        return Ok(());
    }

    let plan = Arc::new(plan);
    let (tx, rx) = mpsc::channel::<Event>();
    let engine_ctx = ctx.clone();
    let engine_plan = plan.clone();
    let start_engine = move || {
        let mut engine = Engine::new(&engine_ctx, &engine_plan, move |ev| {
            let _ = tx.send(ev);
        });
        let mut report = Report::new();
        engine.run(&mut report);
        Ok::<_, anyhow::Error>(report)
    };

    let pkg_det_refs: Vec<(&Context, &Detection)> =
        pkg_detections.iter().map(|(c, d)| (c, d)).collect();

    // If stdin/stdout are not terminals, run directly (no TUI) on the
    // main thread and print the summary — useful for pipe/CI.
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        let report = start_engine()?;
        finish(&report);

        // Post-setup readiness sweep.
        run_readiness_sweep(ctx, &pkg_det_refs, registry);
        return Ok(());
    }

    let report = tui::run(&plan, &rx, yes, start_engine)?;
    finish(&report);

    // Post-setup readiness sweep.
    run_readiness_sweep(ctx, &pkg_det_refs, registry);
    Ok(())
}

// ── upone ready ─────────────────────────────────────────────────────────────

fn cmd_ready(ctx: &Context, registry: &upone_core::Registry) {
    let root = ctx.cwd.clone();
    let mut all_dirs = vec![root.clone()];
    all_dirs.extend(workspace::package_dirs(&root));

    let mut pkg_detections: Vec<(Context, Detection)> = Vec::new();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();

    for dir in &all_dirs {
        let rel_display = dir
            .strip_prefix(&root)
            .ok()
            .filter(|rel| !rel.as_os_str().is_empty())
            .map(|r| r.display().to_string());
        let dir_ctx = Context { cwd: dir.clone() };
        let dir_detections = upone_core::detect::detect(dir, registry);
        for d in dir_detections.found {
            let key = (
                rel_display.clone().unwrap_or_default(),
                d.provider.to_string(),
                d.signature.clone(),
            );
            if seen.insert(key) {
                pkg_detections.push((dir_ctx.clone(), d));
            }
        }
    }

    if pkg_detections.is_empty() {
        report::no_project(ctx);
        return;
    }

    let pkg_det_refs: Vec<(&Context, &Detection)> =
        pkg_detections.iter().map(|(c, d)| (c, d)).collect();

    let checks = collect_readiness_checks(ctx, &pkg_det_refs, registry);
    if checks.is_empty() {
        println!();
        println!("  no readiness checks applicable for detected technologies.");
        println!();
        return;
    }

    let readiness_report = upone_core::sweep(ctx, &checks);
    report::readiness(&readiness_report);

    if !readiness_report.is_ready() {
        std::process::exit(1);
    }
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Runs the readiness sweep and prints the report.
fn run_readiness_sweep(
    ctx: &Context,
    package_detections: &[(&Context, &Detection)],
    registry: &upone_core::Registry,
) {
    let checks = collect_readiness_checks(ctx, package_detections, registry);
    if checks.is_empty() {
        return;
    }
    let readiness_report = upone_core::sweep(ctx, &checks);
    report::readiness(&readiness_report);
}

/// Prints the final summary and exits non-zero when any task failed, so
/// scripts/CI can rely on the exit code.
fn finish(report: &Report) {
    report::summary(report);
    if report.has_error() {
        std::process::exit(1);
    }
}
