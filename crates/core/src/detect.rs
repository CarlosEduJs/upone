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

    /// File signatures that indicate the project (ex: [`["bun.lock"]`]).
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
            reason: format!("found {signature}"),
        }
    }

    /// Builds the tasks that prepare the project. Uses `Planner`.
    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>);

    /// Returns readiness checks this provider wants to verify after setup.
    /// Default: none.
    fn readiness_checks(&self, _ctx: &Context) -> Vec<crate::readiness::ReadinessCheck> {
        Vec::new()
    }
}

/// List of registered providers.
#[derive(Default)]
pub struct Registry {
    providers: Vec<Box<dyn Provider>>,
}

impl Registry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a provider.
    pub fn register(&mut self, p: Box<dyn Provider>) {
        self.providers.push(p);
    }

    #[must_use]
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
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.found.is_empty()
    }
}

/// Runs all providers and collects detections.
#[must_use]
pub fn detect(cwd: &Path, registry: &Registry) -> Detected {
    let mut out = Detected::default();
    for provider in registry.all() {
        if let Some(d) = provider.detect(cwd) {
            out.found.push(d);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    struct MarkerA;
    struct MarkerB;

    impl Provider for MarkerA {
        fn id(&self) -> &'static str {
            "a"
        }
        fn signatures(&self) -> &'static [&'static str] {
            &["a.lock"]
        }
        fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
    }

    impl Provider for MarkerB {
        fn id(&self) -> &'static str {
            "b"
        }
        fn signatures(&self) -> &'static [&'static str] {
            &["b.lock"]
        }
        fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
    }

    #[test]
    fn is_empty_defaults_to_true() {
        assert!(Detected::default().is_empty());
    }

    #[test]
    fn is_empty_false_when_anything_found() {
        let mut detected = Detected::default();
        detected.found.push(Detection {
            provider: "x",
            signature: "x".to_string(),
            reason: "found x".to_string(),
        });
        assert!(!detected.is_empty());
    }

    #[test]
    fn detect_matches_signatures_in_registry() {
        let mut registry = Registry::new();
        registry.register(Box::new(MarkerA));
        registry.register(Box::new(MarkerB));

        let dir = std::env::temp_dir().join(format!("upone-detect-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("b.lock"), "").expect("write file");
        std::fs::write(dir.join("other.txt"), "").expect("write file");
        let detected = detect(&dir, &registry);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(detected.found.len(), 1);
        assert_eq!(detected.found[0].provider, "b");
        assert_eq!(detected.found[0].signature, "b.lock");
    }

    #[test]
    fn detect_returns_nothing_on_empty_dir() {
        let dir = std::env::temp_dir().join(format!("upone-detect-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let detected = detect(&dir, &Registry::new());
        std::fs::remove_dir_all(&dir).ok();
        assert!(detected.is_empty());
    }
}
