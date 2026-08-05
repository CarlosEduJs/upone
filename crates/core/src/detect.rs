//! File-signature detection engine.

use std::path::Path;

use crate::plan::Planner;
use crate::{Context, Detection};

/// A technology Provider.
///
/// The Core never knows specific technologies: it just runs the
/// detection and planning delegated to whoever implements this trait.
pub trait Provider: Send + Sync {
    /// Unique identifier (ex: "bun", "cargo").
    fn id(&self) -> &'static str;

    /// File signatures that indicate the project (ex: ["bun.lock"]).
    fn signatures(&self) -> &'static [&'static str];

    /// Detects the project's presence.
    ///
    /// Default: looks for file signatures. Providers that depend on
    /// content (ex: Postgres inside docker-compose) override this.
    fn detect(&self, cwd: &Path) -> Option<Detection> {
        for sig in self.signatures() {
            let matched = cwd.join(sig);
            if matched.is_file() {
                return Some(self.found(sig));
            }
        }
        None
    }

    /// Builds the detection reason for a matching signature.
    fn found(&self, signature: &str) -> Detection {
        Detection {
            provider: self.id(),
            signature: signature.to_string(),
            reason: format!("found {}", signature),
        }
    }

    /// Builds the tasks that prepare the project. Uses `Planner`.
    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>);
}

/// List of registered providers.
#[derive(Default)]
pub struct Registry {
    providers: Vec<Box<dyn Provider>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a provider.
    pub fn register(&mut self, p: Box<dyn Provider>) {
        self.providers.push(p);
    }

    pub fn all(&self) -> &[Box<dyn Provider>] {
        &self.providers
    }
}

/// Result of the detection sweep.
#[derive(Default)]
pub struct Detected {
    pub found: Vec<Detection>,
}

impl Detected {
    pub fn is_empty(&self) -> bool {
        self.found.is_empty()
    }
}

/// Runs all providers and collects detections.
pub fn detect(cwd: &Path, registry: &Registry) -> Detected {
    let mut out = Detected::default();
    for provider in registry.all() {
        if let Some(d) = provider.detect(cwd) {
            out.found.push(d);
        }
    }
    out
}
