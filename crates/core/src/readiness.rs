//! Environment readiness validation.
//!
//! Lightweight, non-invasive checks that confirm the environment prepared by
//! `upone up` is actually ready for development.  Each check inspects state
//! (socket open, file exists, env var present) rather than running arbitrary
//! commands — the question is "is the environment ready?", not "what commands
//! can I run?".

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use crate::Context;

// ── Importance ──────────────────────────────────────────────────────────────

/// Whether a readiness check is required for development or merely
/// recommended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Importance {
    /// Development cannot proceed without this.
    Required,
    /// Nice to have; a warning is shown when missing.
    Optional,
}

// ── Status ──────────────────────────────────────────────────────────────────

/// Outcome of evaluating a single readiness check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadinessStatus {
    /// The check passed.
    Ready(String),
    /// The check failed on an *optional* requirement — a warning.
    Warning { reason: String, remedy: String },
    /// The check failed on a *required* requirement — the environment is not
    /// ready.
    NotReady { reason: String, remedy: String },
}

impl ReadinessStatus {
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    #[must_use]
    pub const fn is_not_ready(&self) -> bool {
        matches!(self, Self::NotReady { .. })
    }
}

// ── Check ───────────────────────────────────────────────────────────────────

/// Callback that evaluates the check against the project context.
pub type CheckFn = Arc<dyn Fn(&Context) -> ReadinessStatus + Send + Sync>;

/// A single, non-invasive environment readiness check.
pub struct ReadinessCheck {
    /// Machine-friendly identifier (e.g. `"env-DATABASE_URL"`).
    pub id: String,
    /// Short label for terminal output (e.g. `"DATABASE_URL"`).
    pub label: String,
    /// What this check verifies, in plain language.
    pub description: String,
    /// Required or optional.
    pub importance: Importance,
    /// The function that runs the verification.
    pub check_fn: CheckFn,
}

impl ReadinessCheck {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
        importance: Importance,
        check_fn: impl Fn(&Context) -> ReadinessStatus + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: description.into(),
            importance,
            check_fn: Arc::new(check_fn),
        }
    }

    /// Evaluate this check.
    #[must_use]
    pub fn run(&self, ctx: &Context) -> ReadinessStatus {
        (self.check_fn)(ctx)
    }
}

impl fmt::Debug for ReadinessCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadinessCheck")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("importance", &self.importance)
            .finish_non_exhaustive()
    }
}

// ── Report ──────────────────────────────────────────────────────────────────

/// Evaluated result of one check — pairs the check metadata with its status.
#[derive(Debug, Clone)]
pub struct ReadinessResult {
    pub id: String,
    pub label: String,
    pub importance: Importance,
    pub status: ReadinessStatus,
}

/// Aggregated result of running all readiness checks.
#[derive(Debug, Default)]
pub struct ReadinessReport {
    pub results: Vec<ReadinessResult>,
}

impl ReadinessReport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` when every required check passed.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        !self.results.iter().any(|r| r.status.is_not_ready())
    }

    /// All results that are warnings (optional failures).
    #[must_use]
    pub fn warnings(&self) -> Vec<&ReadinessResult> {
        self.results
            .iter()
            .filter(|r| matches!(r.status, ReadinessStatus::Warning { .. }))
            .collect()
    }

    /// All results that are failures (required).
    #[must_use]
    pub fn failures(&self) -> Vec<&ReadinessResult> {
        self.results
            .iter()
            .filter(|r| r.status.is_not_ready())
            .collect()
    }
}

// ── Sweep ───────────────────────────────────────────────────────────────────

/// Runs all readiness checks and returns a report.
#[must_use]
pub fn sweep(ctx: &Context, checks: &[ReadinessCheck]) -> ReadinessReport {
    let mut report = ReadinessReport::new();
    for check in checks {
        let status = check.run(ctx);
        report.results.push(ReadinessResult {
            id: check.id.clone(),
            label: check.label.clone(),
            importance: check.importance,
            status,
        });
    }
    report
}

// ── Env-key helpers ─────────────────────────────────────────────────────────

const DOTENV_FILES: &[&str] = &[".env.local", ".env.development", ".env"];

/// Resolves an environment variable by checking, in order:
/// 1. Process environment (`std::env::var`)
/// 2. `.env.local`
/// 3. `.env.development`
/// 4. `.env`
///
/// The first hit wins (deliberate: `.env.local` overrides the committed
/// `.env` base — the environment-specific value is the one that applies, so
/// a local override beats a stale base value). Returns the value if found in
/// any layer, `None` otherwise.
#[must_use]
pub fn resolve_env_key(cwd: &Path, key: &str) -> Option<String> {
    if let Ok(val) = std::env::var(key) {
        if !val.is_empty() {
            return Some(val);
        }
    }

    for file in DOTENV_FILES {
        let path = cwd.join(file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(val) = parse_dotenv_key(&content, key) {
                return Some(val);
            }
        }
    }

    None
}

/// Minimal dotenv parser: extracts the value for `key` from dotenv content.
/// Handles `KEY=value`, `KEY="value"`, `KEY='value'`, and skips comments/blanks.
fn parse_dotenv_key(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Skip `export` prefix.
        let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);

        if let Some(rest) = trimmed.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(val) = rest.strip_prefix('=') {
                let val = val.trim();
                // Strip surrounding quotes if matching and valid (len >= 2).
                let val = if val.len() >= 2
                    && ((val.starts_with('"') && val.ends_with('"'))
                        || (val.starts_with('\'') && val.ends_with('\'')))
                {
                    &val[1..val.len() - 1]
                } else if val.starts_with('"') || val.starts_with('\'') {
                    // Unmatched or malformed quote (e.g. lone opening quote) -> invalid
                    ""
                } else {
                    val
                };
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

// ── Env-example parser ──────────────────────────────────────────────────────

/// An environment requirement extracted from a `.env.example` or
/// `.env.template` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvRequirement {
    pub key: String,
    pub importance: Importance,
}

/// Well-known template filenames, checked in order.
const TEMPLATE_FILES: &[&str] = &[".env.example", ".env.template"];

/// Parses `.env.example` / `.env.template` and extracts the required keys.
///
/// A key is marked [`Importance::Optional`] when the preceding comment
/// contains `optional` (case-insensitive). All other keys default to
/// [`Importance::Required`].
#[must_use]
pub fn env_requirements_from_template(cwd: &Path) -> Vec<EnvRequirement> {
    for file in TEMPLATE_FILES {
        let path = cwd.join(file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            return parse_env_template(&content);
        }
    }
    Vec::new()
}

fn parse_env_template(content: &str) -> Vec<EnvRequirement> {
    let mut reqs = Vec::new();
    let mut last_comment_optional = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            last_comment_optional = false;
            continue;
        }
        if trimmed.starts_with('#') {
            last_comment_optional = trimmed.to_lowercase().contains("optional");
            continue;
        }

        // Strip optional `export` prefix.
        let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);

        if let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim().to_string();
            if !key.is_empty() {
                let importance = if last_comment_optional {
                    Importance::Optional
                } else {
                    Importance::Required
                };
                reqs.push(EnvRequirement { key, importance });
            }
        }
        last_comment_optional = false;
    }
    reqs
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_dotenv_basic() {
        let content = r#"
DATABASE_URL=postgres://localhost/mydb
# a comment
REDIS_URL="redis://localhost:6379"
export SECRET='my-secret'
EMPTY=
"#;
        assert_eq!(
            parse_dotenv_key(content, "DATABASE_URL"),
            Some("postgres://localhost/mydb".into())
        );
        assert_eq!(
            parse_dotenv_key(content, "REDIS_URL"),
            Some("redis://localhost:6379".into())
        );
        assert_eq!(
            parse_dotenv_key(content, "SECRET"),
            Some("my-secret".into())
        );
        // Empty values should return None.
        assert_eq!(parse_dotenv_key(content, "EMPTY"), None);
        assert_eq!(parse_dotenv_key(content, "MISSING"), None);
    }

    #[test]
    fn parse_dotenv_malformed_quotes() {
        let content = "LONE_QUOTE=\"\nLONE_SINGLE=' \nUNMATCHED=\"foo\nEMPTY_QUOTES=\"\"\n";
        assert_eq!(parse_dotenv_key(content, "LONE_QUOTE"), None);
        assert_eq!(parse_dotenv_key(content, "LONE_SINGLE"), None);
        assert_eq!(parse_dotenv_key(content, "UNMATCHED"), None);
        assert_eq!(parse_dotenv_key(content, "EMPTY_QUOTES"), None);
    }

    #[test]
    fn parse_dotenv_no_false_prefix_match() {
        let content = "DATABASE_URL_SHADOW=shadow\nDATABASE_URL=real\n";
        assert_eq!(
            parse_dotenv_key(content, "DATABASE_URL"),
            Some("real".into())
        );
    }

    #[test]
    fn parse_template_required_and_optional() {
        let content = r"
DATABASE_URL=postgres://localhost/mydb
BETTER_AUTH_SECRET=change-me

# optional
STRIPE_SECRET_KEY=sk_test_xxx
ANALYTICS_KEY=

# This is required
NEXT_PUBLIC_API_URL=https://api.example.com
";
        let reqs = parse_env_template(content);
        assert_eq!(reqs.len(), 5);

        assert_eq!(reqs[0].key, "DATABASE_URL");
        assert_eq!(reqs[0].importance, Importance::Required);

        assert_eq!(reqs[1].key, "BETTER_AUTH_SECRET");
        assert_eq!(reqs[1].importance, Importance::Required);

        assert_eq!(reqs[2].key, "STRIPE_SECRET_KEY");
        assert_eq!(reqs[2].importance, Importance::Optional);

        // ANALYTICS_KEY: the `# optional` comment was consumed by
        // STRIPE_SECRET_KEY, so ANALYTICS_KEY defaults to Required.
        assert_eq!(reqs[3].key, "ANALYTICS_KEY");
        assert_eq!(reqs[3].importance, Importance::Required);

        assert_eq!(reqs[4].key, "NEXT_PUBLIC_API_URL");
        assert_eq!(reqs[4].importance, Importance::Required);
    }

    #[test]
    fn resolve_env_key_from_dotenv_file() {
        let dir = std::env::temp_dir().join(format!("upone-readiness-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join(".env"), "MY_TEST_KEY=from-dotenv\n").unwrap();
        let result = resolve_env_key(&dir, "MY_TEST_KEY");
        assert_eq!(result, Some("from-dotenv".into()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_requirements_from_template_file() {
        let dir = std::env::temp_dir().join(format!("upone-readiness-tpl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join(".env.example"),
            "DATABASE_URL=\n# optional\nSTRIPE_KEY=\n",
        )
        .unwrap();

        let reqs = env_requirements_from_template(&dir);
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].key, "DATABASE_URL");
        assert_eq!(reqs[0].importance, Importance::Required);
        assert_eq!(reqs[1].key, "STRIPE_KEY");
        assert_eq!(reqs[1].importance, Importance::Optional);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_collects_results() {
        let ctx = Context {
            cwd: std::path::PathBuf::from("/tmp/upone-sweep-test"),
        };
        let checks = vec![
            ReadinessCheck::new(
                "ok-check",
                "ok",
                "always ready",
                Importance::Required,
                |_| ReadinessStatus::Ready("all good".into()),
            ),
            ReadinessCheck::new(
                "fail-check",
                "fail",
                "always fails",
                Importance::Required,
                |_| ReadinessStatus::NotReady {
                    reason: "broken".into(),
                    remedy: "fix it".into(),
                },
            ),
            ReadinessCheck::new(
                "warn-check",
                "warn",
                "optional warning",
                Importance::Optional,
                |_| ReadinessStatus::Warning {
                    reason: "missing".into(),
                    remedy: "add it".into(),
                },
            ),
        ];
        let report = sweep(&ctx, &checks);
        assert_eq!(report.results.len(), 3);
        assert!(!report.is_ready());
        assert_eq!(report.failures().len(), 1);
        assert_eq!(report.warnings().len(), 1);
    }
}
