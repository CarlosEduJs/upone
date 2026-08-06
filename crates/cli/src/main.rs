//! `upone` — prepares development environments with a single command.

mod report;
mod tui;

use std::io::IsTerminal;
use std::sync::{mpsc, Arc};

use clap::Parser;
use upone_core::{Context, Engine, Event, Report};
use upone_providers::build_registry;

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
    let detections = upone_core::detect::detect(&ctx.cwd, &registry);

    let mut planner = upone_core::Planner::new(&ctx);
    for d in &detections.found {
        let provider = registry
            .all()
            .iter()
            .find(|p| p.id() == d.provider)
            .expect("provider registered for detection");
        provider.plan(&ctx, &mut planner);
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
