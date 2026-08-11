//! Simple textual output (non-TUI): preview, dry-run and final summary.

use upone_core::detect::Detected;
use upone_core::plan::{Plan, RunOutcome};
use upone_core::{Report, StepStatus};

const CHECK: &str = "\u{2713}";
const CROSS: &str = "\u{2717}";

pub fn no_project(ctx: &upone_core::Context) {
    println!();
    println!(
        "  {} no known technology detected in {}",
        CROSS,
        ctx.cwd.display()
    );
    println!();
    println!("  upone does not know this kind of project yet.");
    println!("  run `upone --help` to learn more.");
    println!();
}

pub fn preview(detections: &Detected, plan: &Plan) {
    println!();
    for d in &detections.found {
        println!(
            "  {} Detected {} ({}) — {}",
            CHECK, d.provider, d.signature, d.reason
        );
    }
    println!("  {CHECK} Plan created");
    println!();

    // groups by level to show the execution order
    for (i, level) in plan.levels.iter().enumerate() {
        let label = if i == 0 { "next" } else { "later" };
        for id in level {
            let Some(task) = plan.task(id) else {
                continue;
            };
            println!(
                "     [{}] {} — {} (risk: {})",
                label,
                task.label,
                task.description,
                task.risk.label()
            );
        }
    }
    println!();
}

pub fn dry_run_done() {
    println!("  nothing was executed (dry-run).");
    println!();
}

pub fn summary(report: &Report) {
    println!();
    let mut ok = 0;
    let mut skipped = 0;
    let mut failed = 0;
    for step in &report.steps {
        match &step.status {
            StepStatus::Done(RunOutcome::Ran) => {
                ok += 1;
                let detail = step.detail.as_deref().unwrap_or("");
                println!(
                    "  {} {} — {}",
                    CHECK,
                    step.label,
                    if detail.is_empty() { "ok" } else { detail }
                );
            }
            StepStatus::Done(RunOutcome::Skipped(reason)) => {
                skipped += 1;
                println!("  {} {} — skipped: {}", CHECK, step.label, reason);
            }
            StepStatus::Error(e) => {
                failed += 1;
                println!("  {} {} — {}", CROSS, step.label, e);
            }
            _ => {}
        }
    }
    println!();
    if failed == 0 {
        println!("  ready: {ok} tasks executed, {skipped} skipped.");
    } else {
        println!("  {failed} tasks failed. Review the messages above.");
    }
    println!();
}

const WARN: &str = "\u{26a0}";

pub fn readiness(report: &upone_core::ReadinessReport) {
    use upone_core::ReadinessStatus;

    if report.results.is_empty() {
        return;
    }

    println!();
    println!("  environment readiness");
    println!("  {}", "─".repeat(40));

    for r in &report.results {
        match &r.status {
            ReadinessStatus::Ready(msg) => {
                println!("  {CHECK} {label} — {msg}", label = r.label);
            }
            ReadinessStatus::Warning { reason, remedy } => {
                println!("  {WARN} {label} — {reason}", label = r.label);
                println!("    {WARN} {remedy}");
            }
            ReadinessStatus::NotReady { reason, remedy } => {
                println!("  {CROSS} {label} — {reason}", label = r.label);
                println!("    {CROSS} {remedy}");
            }
        }
    }

    println!();
    let ready = report
        .results
        .iter()
        .filter(|r| r.status.is_ready())
        .count();
    let warnings = report.warnings().len();
    let failures = report.failures().len();

    if failures == 0 && warnings == 0 {
        println!("  {CHECK} all {ready} checks passed.");
    } else if failures == 0 {
        println!("  {CHECK} {ready} checks passed, {warnings} warnings.");
    } else {
        println!("  {CROSS} {ready} ready, {warnings} warnings, {failures} not ready.");
    }
    println!();
}
