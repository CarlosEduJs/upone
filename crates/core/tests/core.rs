//! Core tests: topological ordering, cycles and detection.

use std::path::PathBuf;

use upone_core::detect::{detect, Registry};
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::run::RunError;
use upone_core::{Context, Provider, Report, Risk};

fn ctx() -> Context {
    Context {
        cwd: PathBuf::from("/tmp/upone-test"),
    }
}

#[test]
fn plan_respects_dependencies() {
    let c = ctx();
    let mut p = Planner::new(&c);
    p.add(
        Task::new("install", "install", "install")
            .depends_on(["check"])
            .risk(Risk::Medium),
    );
    p.add(Task::new("check", "check", "check").risk(Risk::Low));
    p.add(
        Task::new("gen", "gen", "generate")
            .depends_on(["install"])
            .risk(Risk::High),
    );

    let plan = p.build().unwrap();
    // check (level 0) -> install (level 1) -> gen (level 2)
    assert_eq!(plan.levels.len(), 3);
    assert_eq!(plan.levels[0], vec!["check"]);
    assert_eq!(plan.levels[1], vec!["install"]);
    assert_eq!(plan.levels[2], vec!["gen"]);
}

#[test]
fn independent_tasks_share_the_same_level() {
    let c = ctx();
    let mut p = Planner::new(&c);
    p.add(Task::new("a", "a", "a"));
    p.add(Task::new("b", "b", "b"));
    let plan = p.build().unwrap();
    assert_eq!(plan.levels.len(), 1);
    assert_eq!(plan.levels[0], vec!["a", "b"]);
}

#[test]
fn cycle_errors() {
    let c = ctx();
    let mut p = Planner::new(&c);
    p.add(Task::new("a", "a", "a").depends_on(["b"]));
    p.add(Task::new("b", "b", "b").depends_on(["a"]));
    assert!(p.build().is_err(), "a cycle should fail");
}

#[test]
fn missing_dependency_errors() {
    let c = ctx();
    let mut p = Planner::new(&c);
    p.add(Task::new("a", "a", "a").depends_on(["ghost"]));
    assert!(p.build().is_err());
}

#[test]
fn run_emits_event_and_outcome() {
    let c = ctx();
    let mut p = Planner::new(&c);
    p.add(Task::new("a", "a", "a").run(|c, e| run_ok(c, e)));
    let plan = p.build().unwrap();

    let plan = &plan;
    let mut engine = upone_core::Engine::new(&c, plan, |_| {});
    let mut report = Report::new();
    engine.run(&mut report);
    assert!(!report.has_error());
}

fn run_ok(_c: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    emit("ran");
    Ok(RunOutcome::Ran("ok".into()))
}

// ---- detection ----

struct FakeProvider;

impl Provider for FakeProvider {
    fn id(&self) -> &'static str {
        "fake"
    }
    fn signatures(&self) -> &'static [&'static str] {
        &["fake.lock"]
    }
    fn plan(&self, _ctx: &Context, _planner: &mut Planner<'_>) {}
}

#[test]
fn detection_by_signature() {
    let dir = std::env::temp_dir().join("upone-detect-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fake.lock"), "").unwrap();

    let mut reg = Registry::new();
    reg.register(Box::new(FakeProvider));
    let out = detect(&dir, &reg);
    assert_eq!(out.found.len(), 1);
    assert_eq!(out.found[0].provider, "fake");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_detection_without_signature() {
    let dir = std::env::temp_dir().join("upone-detect-empty");
    std::fs::create_dir_all(&dir).unwrap();

    let mut reg = Registry::new();
    reg.register(Box::new(FakeProvider));
    let out = detect(&dir, &reg);
    assert!(out.found.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}
