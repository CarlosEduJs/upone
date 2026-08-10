//! `upone` — prepares development environments with a single command.

#![allow(clippy::print_stdout)]

mod report;
mod tui;

use std::io::IsTerminal;
use std::sync::{mpsc, Arc};

use clap::Parser;
use upone_core::{Context, Detection, Engine, Event, Report};
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
    // `plan_workspace` already prefixes every failure with "failed to build
    // the plan"; keep that message single-source (see workspace.rs).
    let workspace_plan = workspace::plan_workspace(ctx, registry).map_err(anyhow::Error::msg)?;

    if workspace_plan.detections.is_empty() {
        report::no_project(ctx);
        return Ok(());
    }

    report::preview(&workspace_plan.detections, &workspace_plan.plan);

    if dry_run {
        report::dry_run_done();
        return Ok(());
    }

    let plan = Arc::new(workspace_plan.plan);
    let pkg_detections = workspace_plan.package_detections;
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
    let (_, pkg_detections) = workspace::detect_workspace(ctx, registry);

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
