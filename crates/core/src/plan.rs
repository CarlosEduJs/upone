//! Task DAG. The Planner never runs in a fixed order:
//! dependencies determine the order via topological sort.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
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
    /// Directory the task runs in. When `None`, the Planner stamps it with
    /// the project context's cwd (so tasks know where to run, even in a
    /// monorepo where a task may live in a package subfolder).
    pub cwd: Option<PathBuf>,
    pub run: Option<RunFn>,
}

impl Task {
    pub fn new(
        id: impl Into<TaskId>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: description.into(),
            risk: Risk::Low,
            deps: Vec::new(),
            cwd: None,
            run: None,
        }
    }

    #[must_use]
    pub const fn risk(mut self, risk: Risk) -> Self {
        self.risk = risk;
        self
    }

    /// Sets the directory the task runs in. Defaults to the project root.
    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    #[must_use]
    pub fn depends_on(mut self, deps: impl IntoIterator<Item = impl Into<TaskId>>) -> Self {
        self.deps = deps.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
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
    #[must_use]
    pub const fn new(ctx: &'a Context) -> Self {
        Planner {
            ctx,
            tasks: Vec::new(),
        }
    }

    pub fn add(&mut self, task: Task) {
        let task = if task.cwd.is_none() {
            Task {
                cwd: Some(self.ctx.cwd.clone()),
                ..task
            }
        } else {
            task
        };
        self.tasks.push(task);
    }

    /// Resolves the topological order: a list of "levels", where each
    /// level is a set of independent (parallelizable) tasks.
    ///
    /// # Errors
    ///
    /// Returns an error when the plan has duplicate task ids, references an
    /// unknown dependency, or contains a dependency cycle.
    pub fn build(self) -> Result<Plan, String> {
        self.build_inner(false)
    }

    /// Like [`Planner::build`] but tolerates dependencies on tasks that live
    /// outside this planner (e.g. a workspace package depending on the root
    /// `bun-install`). Such external deps are ignored for ordering here and
    /// validated when the plans are merged.
    ///
    /// # Errors
    ///
    /// Returns an error when the plan has duplicate task ids or contains a
    /// dependency cycle.
    pub fn build_allow_external(self) -> Result<Plan, String> {
        self.build_inner(true)
    }

    fn build_inner(self, allow_external: bool) -> Result<Plan, String> {
        let mut dependents: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
        let mut indeg: HashMap<TaskId, usize> = HashMap::new();
        let mut by_id: HashMap<TaskId, Task> = HashMap::new();

        for task in self.tasks {
            if by_id.contains_key(&task.id) {
                return Err(format!("duplicate task id '{}' in the plan", task.id));
            }
            for dep in &task.deps {
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(task.id.clone());
            }
            by_id.insert(task.id.clone(), task);
        }

        for (id, task) in &by_id {
            let known_deps = task
                .deps
                .iter()
                .filter(|d| by_id.contains_key(d.as_str()))
                .count();
            indeg.insert(id.clone(), known_deps);
        }

        if !allow_external {
            for (id, task) in &by_id {
                for dep in &task.deps {
                    if !by_id.contains_key(dep) {
                        return Err(format!(
                            "task '{id}' depends on '{dep}' which does not exist"
                        ));
                    }
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
    #[must_use]
    pub fn task(&self, id: &TaskId) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values()
    }

    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Context;

    fn ctx(cwd: &str) -> Context {
        Context {
            cwd: PathBuf::from(cwd),
        }
    }

    fn plan_with(tasks: Vec<Task>, allow_external: bool) -> Result<Plan, String> {
        let context = ctx("/proj");
        let mut planner = Planner::new(&context);
        for task in tasks {
            planner.add(task);
        }
        if allow_external {
            planner.build_allow_external()
        } else {
            planner.build()
        }
    }

    /// Extracts the error message of a failed build.
    fn build_err(tasks: Vec<Task>, allow_external: bool) -> String {
        match plan_with(tasks, allow_external) {
            Err(e) => e,
            Ok(_) => panic!("expected the plan build to fail"),
        }
    }

    #[test]
    fn duplicate_id_rejected() {
        let tasks = vec![Task::new("x", "x", "x"), Task::new("x", "y", "y")];
        assert!(build_err(tasks, false).contains("duplicate task id 'x'"));
    }

    #[test]
    fn duplicate_id_rejected_even_with_allow_external() {
        let tasks = vec![Task::new("x", "x", "x"), Task::new("x", "y", "y")];
        assert!(build_err(tasks, true).contains("duplicate task id 'x'"));
    }

    #[test]
    fn unknown_dep_rejected() {
        let tasks = vec![Task::new("a", "a", "a").depends_on(["missing"])];
        assert!(build_err(tasks, false).contains("depends on 'missing'"));
    }

    #[test]
    fn unknown_dep_allowed_with_allow_external() {
        let tasks = vec![
            Task::new("a", "a", "a").depends_on(["external"]),
            Task::new("b", "b", "b"),
        ];
        assert!(plan_with(tasks, true).is_ok());
    }

    #[test]
    fn cycle_rejected() {
        let tasks = vec![
            Task::new("a", "a", "a").depends_on(["b"]),
            Task::new("b", "b", "b").depends_on(["a"]),
        ];
        assert!(build_err(tasks, false).contains("cycle"));
    }

    #[test]
    fn topological_order_respected() {
        let tasks = vec![
            Task::new("a", "a", "a").depends_on(["b", "c"]),
            Task::new("b", "b", "b").depends_on(["d"]),
            Task::new("c", "c", "c"),
            Task::new("d", "d", "d"),
            Task::new("e", "e", "e").depends_on(["a"]),
        ];
        let plan = plan_with(tasks, false).unwrap();
        assert_eq!(plan.ids().len(), 5);

        let index = |id: &str| -> usize {
            plan.levels
                .iter()
                .enumerate()
                .find_map(|(i, level)| level.contains(&id.to_string()).then_some(i))
                .expect("task in plan")
        };
        assert!(index("d") < index("b"));
        assert!(index("b") < index("a"));
        assert!(index("c") < index("a"));
        assert!(index("a") < index("e"));
    }

    #[test]
    fn independent_tasks_share_a_level() {
        let tasks = vec![Task::new("a", "a", "a"), Task::new("b", "b", "b")];
        let plan = plan_with(tasks, false).unwrap();
        assert_eq!(plan.levels.len(), 1);
    }

    #[test]
    fn omit_cwd_stamps_project_context() {
        let context = ctx("/proj");
        let mut planner = Planner::new(&context);
        planner.add(Task::new("a", "a", "a"));
        let plan = planner.build().unwrap();
        assert_eq!(
            plan.task(&"a".into()).unwrap().cwd,
            Some(PathBuf::from("/proj"))
        );
    }
}
