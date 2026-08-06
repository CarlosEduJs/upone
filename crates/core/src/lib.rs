//! Upone Core — the CLI engine.
//!
//! The Core knows no specific technology. It only:

use std::path::PathBuf;

/// Context of the project being prepared.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Project root directory.
    pub cwd: PathBuf,
}

/// Risk level of a task — shown in the plan so the user can decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// No meaningful side effects.
    Low,
    /// May install packages or download from the network.
    Medium,
    /// May alter global state (database, docker, migrations).
    High,
}

impl Risk {
    pub fn label(&self) -> &'static str {
        match self {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
        }
    }
}

/// Result of a Provider detecting the project.
#[derive(Debug, Clone)]
pub struct Detection {
    /// Provider id (ex: "bun").
    pub provider: &'static str,
    /// File that triggered the detection (ex: "bun.lock").
    pub signature: String,
    /// What it means, so the user understands.
    pub reason: String,
}

pub mod detect;
pub mod plan;
pub mod readiness;
pub mod run;

pub use detect::{detect, Detected, Provider, Registry};
pub use plan::{Plan, Planner, RunOutcome, Task, TaskId};
pub use readiness::{
    env_requirements_from_template, resolve_env_key, sweep, EnvRequirement, Importance,
    ReadinessCheck, ReadinessReport, ReadinessResult, ReadinessStatus,
};
pub use run::{Engine, Event, Report, RunError, Step, StepStatus};
