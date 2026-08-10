//! Plan execution engine. Runs levels in order, each level in
//! parallel, and keeps the Report to explain decisions.

use crate::plan::{Plan, RunOutcome, TaskId};
use crate::{Context, Risk};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("task failed: {0}")]
    Failed(String),
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
}

/// An executed step, for the Report/UX.
#[derive(Debug, Clone)]
pub struct Step {
    pub task_id: TaskId,
    pub label: String,
    pub description: String,
    pub risk: Risk,
    pub status: StepStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Running,
    Done(RunOutcome),
    Error(String),
}

/// Final report: everything that happened (Explain > Hide).
#[derive(Debug, Default)]
pub struct Report {
    pub steps: Vec<Step>,
}

impl Report {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn has_error(&self) -> bool {
        self.steps
            .iter()
            .any(|s| matches!(s.status, StepStatus::Error(_)))
    }

    #[must_use]
    pub fn errors(&self) -> Vec<&Step> {
        self.steps
            .iter()
            .filter(|s| matches!(s.status, StepStatus::Error(_)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, status: StepStatus) -> Step {
        Step {
            task_id: id.to_string(),
            label: id.to_string(),
            description: String::new(),
            risk: Risk::Low,
            status,
            detail: None,
        }
    }

    #[test]
    fn errors_filters_error_steps() {
        let mut report = Report::new();
        report
            .steps
            .push(step("ok", StepStatus::Done(RunOutcome::Ran("ok".into()))));
        report.steps.push(step("running", StepStatus::Running));
        report
            .steps
            .push(step("boom", StepStatus::Error("bad".to_string())));

        let errors = report.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].task_id, "boom");
        assert!(report.has_error());
    }

    #[test]
    fn errors_empty_when_nothing_failed() {
        let mut report = Report::new();
        report.steps.push(step(
            "ok",
            StepStatus::Done(RunOutcome::Skipped("ok".into())),
        ));
        assert!(report.errors().is_empty());
        assert!(!report.has_error());
    }

    #[test]
    fn error_detail_is_preserved() {
        let mut report = Report::new();
        report
            .steps
            .push(step("boom", StepStatus::Error("bad".to_string())));
        assert_eq!(
            report.errors()[0].status,
            StepStatus::Error("bad".to_string())
        );
    }
}

/// Progress event emitted during execution.
#[derive(Debug)]
pub enum Event {
    StepStarting(String, String),
    StepDone(Step),
}

pub struct Engine<'a> {
    pub ctx: &'a Context,
    pub plan: &'a Plan,
    on_event: Box<dyn FnMut(Event) + Send + 'a>,
}

impl<'a> Engine<'a> {
    pub fn new(ctx: &'a Context, plan: &'a Plan, on_event: impl FnMut(Event) + Send + 'a) -> Self {
        Engine {
            ctx,
            plan,
            on_event: Box::new(on_event),
        }
    }

    /// Runs the plan: one level at a time; within a level, independent
    /// tasks run in parallel. A failure does not abort the others.
    pub fn run(&mut self, report: &mut Report) {
        for level in &self.plan.levels {
            self.run_level(level, report);
        }
    }

    fn run_level(&mut self, level: &[TaskId], report: &mut Report) {
        // One Step per task: the first pass pushes it as "running" so the
        // report reflects it immediately, and the join pass replaces that
        // same entry with the final status (preserving risk/description).
        let mut entries = Vec::new();
        for id in level {
            if let Some(task) = self.plan.task(id) {
                let step = Step {
                    task_id: task.id.clone(),
                    label: task.label.clone(),
                    description: task.description.clone(),
                    risk: task.risk,
                    status: StepStatus::Running,
                    detail: None,
                };
                report.steps.push(step);
                let step_idx = report.steps.len() - 1;
                (self.on_event)(Event::StepStarting(task.id.clone(), task.label.clone()));

                let ctx = self.ctx.clone();
                let run = task.run.clone();
                let cwd = task.cwd.clone();
                let description = task.description.clone();
                let risk = task.risk;
                entries.push((
                    step_idx,
                    task.id.clone(),
                    task.label.clone(),
                    description,
                    risk,
                    std::thread::spawn(move || {
                        let mut emitted: Vec<String> = Vec::new();
                        let mut emit = |line: &str| emitted.push(line.to_string());
                        let run_ctx = cwd.map_or(ctx, |dir| Context { cwd: dir });
                        let outcome = run.map_or_else(
                            || Ok(RunOutcome::Ran("no action".to_string())),
                            |run| run(&run_ctx, &mut emit),
                        );
                        (outcome, emitted)
                    }),
                ));
            }
        }

        for (step_idx, id, label, description, risk, handle) in entries {
            let (status, detail) = match handle.join() {
                Ok((outcome, emitted)) => {
                    let detail = if emitted.is_empty() {
                        None
                    } else {
                        Some(emitted.join("\n"))
                    };
                    let status = match outcome {
                        Ok(outcome) => StepStatus::Done(outcome),
                        Err(e) => StepStatus::Error(e.to_string()),
                    };
                    (status, detail)
                }
                Err(_) => (StepStatus::Error("task thread panicked".to_string()), None),
            };
            let step = Step {
                task_id: id,
                label,
                description,
                risk,
                status,
                detail,
            };
            report.steps[step_idx] = step.clone();
            (self.on_event)(Event::StepDone(step));
        }
    }
}
