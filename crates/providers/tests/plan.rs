//! Provider plan tests: assert the tasks each provider emits (ids, deps,
//! risk) and the install/database edges wired by the providers.

#![allow(clippy::unwrap_used)]

use upone_core::{detect::detect, Context, Plan, Planner, Risk};
use upone_providers::build_registry;

/// Builds a package.json manifest with the given dependencies.
fn manifest(deps: &[(&str, &str)]) -> String {
    let mut map = serde_json::Map::new();
    for (k, v) in deps {
        map.insert((*k).to_string(), serde_json::json!(v));
    }
    serde_json::json!({ "dependencies": map }).to_string()
}

fn in_dir(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("upone-plan-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (path, content) in files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, content).unwrap();
    }
    dir
}

/// Detects every provider in `dir` and merges their plans into one.
fn planned(dir: &std::path::Path) -> Plan {
    let ctx = Context {
        cwd: dir.to_path_buf(),
    };
    let registry = build_registry();
    let detections = detect(dir, &registry);
    let mut planner = Planner::new(&ctx);
    for d in &detections.found {
        if let Some(provider) = registry.all().iter().find(|p| p.id() == d.provider) {
            provider.plan(&ctx, &mut planner);
        }
    }
    planner.build().unwrap()
}

fn task_ids(plan: &Plan) -> Vec<String> {
    let mut ids = plan.ids();
    ids.sort();
    ids
}

fn deps(plan: &Plan, id: &str) -> Vec<String> {
    let mut deps = plan.task(&id.into()).unwrap().deps.clone();
    deps.sort();
    deps
}

fn risk(plan: &Plan, id: &str) -> Risk {
    plan.task(&id.into()).unwrap().risk
}

/// Runs `f` with a clean environment, free of the connection-string vars that
/// provider detection reads first (`os::env` outranks `.env` files). Guards against
/// an ambient `DATABASE_URL`/`REDIS_URL` in the runner's shell leaking into
/// these fixtures, which would flip which provider/edge is detected.
fn with_clean_db_env<T>(f: impl FnOnce() -> T) -> T {
    const KEYS: [&str; 2] = ["DATABASE_URL", "REDIS_URL"];
    let saved: Vec<(String, String)> = KEYS
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_string(), v)))
        .collect();
    for (k, _) in &saved {
        std::env::remove_var(k);
    }
    let out = f();
    for (k, v) in saved {
        std::env::set_var(k, v);
    }
    out
}

#[test]
fn cargo_plan_emits_check_and_build() {
    let dir = in_dir("cargo", &[("Cargo.toml", "[package]\nname = \"x\"\n")]);
    let plan = planned(&dir);
    assert_eq!(task_ids(&plan), ["cargo-build", "cargo-check"]);
    assert_eq!(deps(&plan, "cargo-build"), ["cargo-check"]);
    assert_eq!(risk(&plan, "cargo-check"), Risk::Low);
    assert_eq!(risk(&plan, "cargo-build"), Risk::Medium);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typeorm_plan_wires_install_and_db_edges() {
    let pkg = manifest(&[("typeorm", "0.3.0")]);
    let dir = in_dir(
        "typeorm",
        &[
            ("package.json", pkg.as_str()),
            ("package-lock.json", "{}"),
            ("data-source.ts", "export const x = 1;"),
            ("compose.yml", "services:\n  db:\n    image: postgres:16\n"),
        ],
    );
    let plan = planned(&dir);
    assert!(task_ids(&plan).contains(&"typeorm-migrate".to_string()));
    assert_eq!(risk(&plan, "typeorm-migrate"), Risk::High);
    assert_eq!(
        deps(&plan, "typeorm-migrate"),
        ["npm-install", "postgres-up", "typeorm-check"]
    );
    // The check itself also waits for the install, and the plan resolves both
    // the install and the DB tasks from the same providers.
    assert_eq!(deps(&plan, "typeorm-check"), ["npm-install"]);
    assert!(task_ids(&plan).contains(&"postgres-up".to_string()));
    assert!(task_ids(&plan).contains(&"docker-up".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn prisma_plan_wires_install_edge_but_no_db() {
    let dir = in_dir(
        "prisma",
        &[
            ("prisma/schema.prisma", "datasource db {\n}\n"),
            ("bun.lock", ""),
        ],
    );
    let plan = planned(&dir);
    assert_eq!(
        deps(&plan, "prisma-generate"),
        ["bun-install", "prisma-check"]
    );
    assert_eq!(risk(&plan, "prisma-generate"), Risk::Medium);
    assert!(task_ids(&plan).contains(&"bun-install".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn alembic_plan_wires_python_install_and_db_edges() {
    let dir = in_dir(
        "alembic",
        &[
            ("alembic.ini", ""),
            ("requirements.txt", "alembic\n"),
            ("compose.yml", "services:\n  db:\n    image: postgres:16\n"),
        ],
    );
    let plan = planned(&dir);
    assert_eq!(risk(&plan, "alembic-upgrade"), Risk::High);
    assert_eq!(
        deps(&plan, "alembic-upgrade"),
        ["alembic-check", "pip-install", "postgres-up"]
    );
    // pip provider emits venv creation before install.
    assert!(task_ids(&plan).contains(&"pip-venv".to_string()));
    assert_eq!(deps(&plan, "pip-install"), ["pip-venv"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn redis_plan_depends_on_docker_up_only_in_compose() {
    with_clean_db_env(|| {
        let dir = in_dir(
            "redis-compose",
            &[("compose.yml", "services:\n  cache:\n    image: redis:7\n")],
        );
        let plan = planned(&dir);
        assert_eq!(deps(&plan, "redis-up"), ["docker-up"]);
        assert_eq!(risk(&plan, "redis-up"), Risk::Low);
        let _ = std::fs::remove_dir_all(&dir);

        let dir = in_dir("redis-standalone", &[("redis.conf", "# redis\n")]);
        let plan = planned(&dir);
        assert!(deps(&plan, "redis-up").is_empty());
        assert!(!task_ids(&plan).contains(&"docker-up".to_string()));
        let _ = std::fs::remove_dir_all(&dir);

        // Configurable via REDIS_URL: no compose, no docker dependency, and the
        // task becomes a URI verification.
        let dir = in_dir(
            "redis-uri",
            &[(".env", "REDIS_URL=redis://mycache:6380/0\n")],
        );
        let plan = planned(&dir);
        assert!(deps(&plan, "redis-up").is_empty());
        assert!(!task_ids(&plan).contains(&"docker-up".to_string()));
        assert_eq!(
            plan.task(&"redis-up".into()).unwrap().label,
            "verify redis URI"
        );
        let _ = std::fs::remove_dir_all(&dir);
    });
}

#[test]
fn postgres_plan_depends_on_docker_up_only_in_compose() {
    with_clean_db_env(|| {
        let dir = in_dir(
            "pg-compose",
            &[("compose.yml", "services:\n  db:\n    image: postgres:16\n")],
        );
        let plan = planned(&dir);
        assert_eq!(deps(&plan, "postgres-up"), ["docker-up"]);
        let _ = std::fs::remove_dir_all(&dir);

        // Detectable via DATABASE_URL only: no compose, no docker dependency.
        let dir = in_dir(
            "pg-env",
            &[(".env", "DATABASE_URL=postgres://localhost/app\n")],
        );
        let plan = planned(&dir);
        assert!(deps(&plan, "postgres-up").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    });
}
