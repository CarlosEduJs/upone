//! Provider detection tests (npm, pnpm, docker, prisma, drizzle,
//! postgres by content, redis by content).

use upone_core::detect::detect;
use upone_providers::build_registry;

fn in_dir(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("upone-prov-{name}"));
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

fn ids(dir: &std::path::Path) -> Vec<String> {
    let reg = build_registry();
    detect(dir, &reg)
        .found
        .into_iter()
        .map(|d| d.provider.to_string())
        .collect()
}

#[test]
fn detects_js_lockfiles() {
    let dir = in_dir("npm", &[("package-lock.json", "{}")]);
    assert!(ids(&dir).contains(&"npm".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir("pnpm", &[("pnpm-lock.yaml", "lockfileVersion: '9.0'")]);
    assert!(ids(&dir).contains(&"pnpm".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_docker_compose() {
    let dir = in_dir(
        "docker",
        &[(
            "docker-compose.yml",
            "services:\n  app:\n    image: nginx\n",
        )],
    );
    assert!(ids(&dir).contains(&"docker".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_prisma_and_drizzle() {
    let dir = in_dir(
        "prisma",
        &[
            (
                "prisma/schema.prisma",
                "datasource db { provider = \"postgresql\" }\n",
            ),
            ("package.json", "{}"),
        ],
    );
    assert!(ids(&dir).contains(&"prisma".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "drizzle",
        &[
            (
                "drizzle.config.ts",
                "export default { schema: './db/schema.ts' }",
            ),
            ("package.json", "{}"),
        ],
    );
    assert!(ids(&dir).contains(&"drizzle".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_postgres_by_compose() {
    let dir = in_dir(
        "pg",
        &[(
            "docker-compose.yml",
            "services:\n  postgres:\n    image: postgres:16\n",
        )],
    );
    assert!(ids(&dir).contains(&"postgres".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_postgres_by_database_url() {
    let dir = in_dir(
        "pgurl",
        &[(
            ".env",
            "DATABASE_URL=postgres://user:pass@localhost:5432/db",
        )],
    );
    assert!(ids(&dir).contains(&"postgres".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_redis_by_compose() {
    let dir = in_dir(
        "redis",
        &[(
            "docker-compose.yml",
            "services:\n  redis:\n    image: redis:7\n",
        )],
    );
    assert!(ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn does_not_detect_postgres_without_signature() {
    let dir = in_dir(
        "nopg",
        &[(
            "docker-compose.yml",
            "services:\n  web:\n    image: nginx\n",
        )],
    );
    assert!(!ids(&dir).contains(&"postgres".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}
