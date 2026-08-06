//! Provider detection tests (npm, pnpm, docker, prisma, drizzle,
//! postgres by content, redis by content).

#![allow(unused_crate_dependencies)]

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

#[test]
fn prisma_custom_output_readiness_check() {
    use upone_core::detect::Provider;
    use upone_core::Context;
    use upone_providers::prisma::Prisma;

    let dir = in_dir(
        "prisma-custom",
        &[
            (
                "prisma/schema.prisma",
                "generator client {\n  provider = \"prisma-client-js\"\n  output = \"../src/generated/client\"\n}\n",
            ),
            ("src/generated/client/index.js", "// generated client"),
        ],
    );
    let ctx = Context { cwd: dir.clone() };
    let p = Prisma;
    let checks = p.readiness_checks(&ctx);
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].id, "prisma-client");

    let status = (checks[0].check_fn)(&ctx);
    assert!(status.is_ready());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn collect_readiness_checks_package_scoped() {
    use upone_core::detect::detect;
    use upone_core::Context;
    use upone_providers::collect_readiness_checks;

    let root = in_dir(
        "ws-readiness",
        &[
            ("package.json", "{\"workspaces\":[\"packages/*\"]}"),
            (
                "packages/db/prisma/schema.prisma",
                "datasource db { provider = \"postgresql\" }\n",
            ),
            (
                "packages/db/packages/db/node_modules/.prisma/client/index.js",
                "// client",
            ),
        ],
    );

    let reg = build_registry();
    let root_ctx = Context { cwd: root.clone() };
    let db_dir = root.join("packages/db");
    let db_ctx = Context {
        cwd: db_dir.clone(),
    };

    let db_detections = detect(&db_dir, &reg);
    let pkg_dets = db_detections
        .found
        .iter()
        .map(|d| (&db_ctx, d))
        .collect::<Vec<_>>();

    let checks = collect_readiness_checks(&root_ctx, &pkg_dets, &reg);
    assert!(checks.iter().any(|c| c.id.starts_with("packages_db-")));
    let _ = std::fs::remove_dir_all(&root);
}
