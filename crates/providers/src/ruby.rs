//! Ruby provider: detects a Gemfile and runs `bundle install`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::{Context, Detection, Risk};

use crate::cmd::{spawn_cmd, which};

pub struct Ruby;

impl Provider for Ruby {
    fn id(&self) -> &'static str {
        "ruby"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["Gemfile"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        if cwd.join("Gemfile.lock").is_file() {
            Some(self.found("Gemfile.lock"))
        } else if cwd.join("Gemfile").is_file() {
            Some(self.found("Gemfile"))
        } else {
            None
        }
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "bundle-check",
            "check ruby/bundler installed",
            "checks that ruby and bundler are on PATH",
        )
        .risk(Risk::Low)
        .run(check_bundle);

        let install = Task::new(
            "bundle-install",
            "bundle install",
            "installs the gems declared in the Gemfile (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["bundle-check"])
        .run(bundle_install);

        planner.add(check);
        planner.add(install);
    }

    fn readiness_checks(&self, _ctx: &Context) -> Vec<ReadinessCheck> {
        vec![ReadinessCheck::new(
            "bundle-available",
            "bundler on PATH",
            "bundle is on PATH so `bundle install` can run",
            Importance::Required,
            |_ctx| {
                if which("bundle") {
                    ReadinessStatus::Ready("bundle found".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "bundle not found on PATH".into(),
                        remedy: "Install bundler with 'gem install bundler' or 'upone up'".into(),
                    }
                }
            },
        )]
    }
}

fn check_bundle(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("ruby") {
        return Err(RunError::Failed(
            "ruby not found on PATH. Install it via https://www.ruby-lang.org/en/downloads/".into(),
        ));
    }
    emit("ruby found on PATH");
    if which("bundle") {
        emit("bundle (bundler) found on PATH");
        Ok(RunOutcome::Ran)
    } else {
        Err(RunError::Failed(
            "bundle (Bundler) not found on PATH. Install it with `gem install bundler`.".into(),
        ))
    }
}

fn bundle_install(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    spawn_cmd("bundle", &["install"], &ctx.cwd, emit)
}
