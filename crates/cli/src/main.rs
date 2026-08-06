//! `upone` — prepares development environments with a single command.

mod report;
mod tui;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::{mpsc, Arc};

use clap::Parser;
use upone_core::{Context, Detection, Detected, Engine, Event, Report, Task};
use upone_providers::{build_registry, workspace};

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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let (dry_run, yes) = match cli.command {
        Command::Up { dry_run, yes } => (dry_run, yes),
    };

    let ctx = Context { cwd };

    let registry = build_registry();

    // Monorepos: detect at the root and at every workspace package, so a
    // project where drizzle lives under `packages/db` is still recognized.
    let root = ctx.cwd.clone();
    let mut all_dirs = vec![root.clone()];
    all_dirs.extend(workspace::package_dirs(&root));

    let mut detections = Detected::default();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut planner = upone_core::Planner::new(&ctx);
    for dir in &all_dirs {
        let slug = dir
            .strip_prefix(&root)
            .ok()
            .filter(|rel| !rel.as_os_str().is_empty())
            .map(dir_slug);
        let dir_ctx = Context { cwd: dir.clone() };

        // Plan this directory's providers with its own cwd so tasks built
        // here know where to run (e.g. `drizzle-kit generate` in packages/db).
        let dir_detections = upone_core::detect::detect(dir, &registry);
        let mut sub_planner = upone_core::Planner::new(&dir_ctx);
        for d in &dir_detections.found {
            let provider = registry
                .all()
                .iter()
                .find(|p| p.id() == d.provider)
                .expect("provider registered for detection");
            provider.plan(&dir_ctx, &mut sub_planner);
        }
        // Relaxed: a package may depend on the root install task (bun-install),
        // which is validated once all plans are merged below.
        let local_plan = sub_planner
            .build_allow_external()
            .map_err(|e| anyhow::anyhow!("failed to build the plan: {e}"))?;

        // Surface detections with their package location in the reason.
        for d in &dir_detections.found {
            if seen.contains(&(d.provider.to_string(), d.signature.clone())) {
                continue;
            }
            seen.insert((d.provider.to_string(), d.signature.clone()));
            let d = d.clone();
            let reason = match &slug {
                Some(s) => format!("{} ({})", d.reason, s.replace('_', "/")),
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
            let task = local_plan.task(&id).cloned().expect("task in plan");
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
        report::no_project(&ctx);
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

    // If stdin/stdout are not terminals, run directly (no TUI) on the
    // main thread and print the summary — useful for pipe/CI.
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        let report = start_engine()?;
        return finish(&report);
    }

    let report = tui::run(&plan, &rx, yes, start_engine)?;
    finish(&report)
}

/// Prints the final summary and exits non-zero when any task failed, so
/// scripts/CI can rely on the exit code.
fn finish(report: &upone_core::Report) -> anyhow::Result<()> {
    report::summary(report);
    if report.has_error() {
        std::process::exit(1);
    }
    Ok(())
}

/// Turns a relative package path into a stable task-id namespace
/// ("packages/db" -> "packages_db").
fn dir_slug(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("_")
}
