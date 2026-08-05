//! Task DAG. The Planner never runs in a fixed order:
//! dependencies determine the order via topological sort.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::{Context, Risk};

pub type TaskId = String;

/// Result of running a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// Actually ran.
    Ran(String),
    /// Skipped for idempotent reasons (already ready).
    Skipped(String),
}

/// Signature of a task's run function.
/// Takes the context and a progress emitter (terminal lines).
pub type RunFn = Arc<
    dyn Fn(&Context, &mut dyn FnMut(&str)) -> Result<RunOutcome, crate::run::RunError>
        + Send
        + Sync,
>;

/// A task in the plan.
#[derive(Clone)]
pub struct Task {
    pub id: TaskId,
    /// Short label for the terminal (ex: "bun install").
    pub label: String,
    /// What it does, so the UX can explain.
    pub description: String,
    pub risk: Risk,
    /// Dependencies by id — determine execution order.
    pub deps: Vec<TaskId>,
    pub run: Option<RunFn>,
}

impl Task {
    pub fn new(
        id: impl Into<TaskId>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Task {
            id: id.into(),
            label: label.into(),
            description: description.into(),
            risk: Risk::Low,
            deps: Vec::new(),
            run: None,
        }
    }

    pub fn risk(mut self, risk: Risk) -> Self {
        self.risk = risk;
        self
    }

    pub fn depends_on(mut self, deps: impl IntoIterator<Item = impl Into<TaskId>>) -> Self {
        self.deps = deps.into_iter().map(Into::into).collect();
        self
    }

    pub fn run(
        mut self,
        run: impl Fn(&Context, &mut dyn FnMut(&str)) -> Result<RunOutcome, crate::run::RunError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        self.run = Some(Arc::new(run));
        self
    }
}

/// Plan builder.
pub struct Planner<'a> {
    pub ctx: &'a Context,
    tasks: Vec<Task>,
}

impl<'a> Planner<'a> {
    pub fn new(ctx: &'a Context) -> Self {
        Planner {
            ctx,
            tasks: Vec::new(),
        }
    }

    pub fn add(&mut self, task: Task) {
        self.tasks.push(task);
    }

    /// Resolves the topological order: a list of "levels", where each
    /// level is a set of independent (parallelizable) tasks.
    pub fn build(self) -> Result<Plan, String> {
        let mut dependents: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
        let mut indeg: HashMap<TaskId, usize> = HashMap::new();
        let mut by_id: HashMap<TaskId, Task> = HashMap::new();

        for task in self.tasks {
            indeg.insert(task.id.clone(), task.deps.len());
            for dep in &task.deps {
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(task.id.clone());
            }
            by_id.insert(task.id.clone(), task);
        }

        for (id, task) in &by_id {
            for dep in &task.deps {
                if !by_id.contains_key(dep) {
                    return Err(format!(
                        "task '{}' depends on '{}' which does not exist",
                        id, dep
                    ));
                }
            }
        }

        let mut ready: Vec<TaskId> = by_id
            .iter()
            .filter(|(k, _)| indeg.get(k.as_str()) == Some(&0))
            .map(|(k, _)| k.clone())
            .collect();

        let mut levels: Vec<Vec<TaskId>> = Vec::new();
        let mut done: HashSet<TaskId> = HashSet::new();

        while !ready.is_empty() {
            ready.sort();
            let level = ready.clone();
            levels.push(level);
            let mut next: Vec<TaskId> = Vec::new();
            for id in &ready {
                done.insert(id.clone());
                if let Some(children) = dependents.get(id) {
                    for c in children {
                        if let Some(count) = indeg.get_mut(c) {
                            *count -= 1;
                            if *count == 0 && !done.contains(c) {
                                next.push(c.clone());
                            }
                        }
                    }
                }
            }
            ready = next;
        }

        if done.len() != by_id.len() {
            return Err("cycle detected in the plan (circular dependencies)".into());
        }

        Ok(Plan {
            levels,
            tasks: by_id,
        })
    }
}

/// Resolved topological order.
pub struct Plan {
    pub levels: Vec<Vec<TaskId>>,
    tasks: HashMap<TaskId, Task>,
}

impl Plan {
    pub fn task(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values()
    }

    pub fn ids(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }
}
