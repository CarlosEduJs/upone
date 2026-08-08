//! PHP provider: detects composer.json / composer.lock and runs
//! `composer install`.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::{Context, Detection, Risk};

use crate::cmd::{spawn_cmd, which};

pub struct Php;

impl Provider for Php {
    fn id(&self) -> &'static str {
        "php"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &["composer.json"]
    }

    fn detect(&self, cwd: &Path) -> Option<Detection> {
        if cwd.join("composer.lock").is_file() {
            Some(self.found("composer.lock"))
        } else if cwd.join("composer.json").is_file() {
            Some(self.found("composer.json"))
        } else {
            None
        }
    }

    fn plan(&self, _ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "php-check",
            "check php/composer installed",
            "checks that php and composer are on PATH",
        )
        .risk(Risk::Low)
        .run(check_php);

        let install = Task::new(
            "composer-install",
            "composer install",
            "installs the dependencies declared in composer.json (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["php-check"])
        .run(composer_install);

        planner.add(check);
        planner.add(install);
    }

    fn readiness_checks(&self, _ctx: &Context) -> Vec<ReadinessCheck> {
        vec![
            ReadinessCheck::new(
                "php-available",
                "php on PATH",
                "php runtime is available",
                Importance::Required,
                |_ctx| {
                    if which("php") {
                        ReadinessStatus::Ready("php found".into())
                    } else {
                        ReadinessStatus::NotReady {
                            reason: "php not found on PATH".into(),
                            remedy: "Install PHP via https://www.php.net/downloads".into(),
                        }
                    }
                },
            ),
            ReadinessCheck::new(
                "composer-available",
                "composer on PATH",
                "composer CLI is available",
                Importance::Required,
                |_ctx| {
                    if which("composer") {
                        ReadinessStatus::Ready("composer found".into())
                    } else {
                        ReadinessStatus::NotReady {
                            reason: "composer not found on PATH".into(),
                            remedy: "Install composer via https://getcomposer.org".into(),
                        }
                    }
                },
            ),
        ]
    }
}

fn check_php(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("php") {
        return Err(RunError::Failed(
            "php not found on PATH. Install it via https://www.php.net/downloads".into(),
        ));
    }
    emit("php found on PATH");
    if which("composer") {
        emit("composer found on PATH");
        Ok(RunOutcome::Ran("composer installed".into()))
    } else {
        Err(RunError::Failed(
            "composer not found on PATH. Install it via https://getcomposer.org/download/".into(),
        ))
    }
}

fn composer_install(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    spawn_cmd("composer", &["install"], &ctx.cwd, emit)
}
