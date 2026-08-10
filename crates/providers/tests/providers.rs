//! Provider detection tests (npm, pnpm, docker, prisma, drizzle,
//! postgres by content, redis by content).

#![allow(clippy::unwrap_used)]

use upone_core::detect::detect;
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
    let dir = std::env::temp_dir().join(format!("upone-prov-{name}-{}", std::process::id()));
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
fn detects_mysql_by_compose_and_env() {
    let dir = in_dir(
        "mysql",
        &[(
            "docker-compose.yml",
            "services:\n  mysql:\n    image: mysql:8\n",
        )],
    );
    assert!(ids(&dir).contains(&"mysql".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // MariaDB is handled by the same provider.
    let dir = in_dir(
        "mariadb",
        &[(
            "docker-compose.yml",
            "services:\n  db:\n    image: mariadb:11\n",
        )],
    );
    assert!(ids(&dir).contains(&"mysql".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "mysqlurl",
        &[(".env", "DATABASE_URL=mysql://user:pass@localhost:3306/db")],
    );
    assert!(ids(&dir).contains(&"mysql".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn does_not_detect_mysql_without_signature() {
    let dir = in_dir(
        "nomysql",
        &[(
            "docker-compose.yml",
            "services:\n  web:\n    image: nginx\n",
        )],
    );
    assert!(!ids(&dir).contains(&"mysql".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_mongo_by_compose_and_uri() {
    let dir = in_dir(
        "mongo",
        &[(
            "docker-compose.yml",
            "services:\n  mongo:\n    image: mongo:7\n",
        )],
    );
    assert!(ids(&dir).contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "mongouri",
        &[(".env", "MONGODB_URI=mongodb://localhost:27017/app")],
    );
    assert!(ids(&dir).contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // mongodb+srv:// (Atlas) URIs also count.
    let dir = in_dir(
        "mongo-srv",
        &[(
            ".env.local",
            "MONGO_URI=mongodb+srv://cluster.example.com/app",
        )],
    );
    assert!(ids(&dir).contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn does_not_detect_mongo_when_value_merely_contains_mongodb() {
    // A value that only *contains* the word "mongodb" — or uses another
    // scheme — must not count as a mongodb connection string.
    let dir = in_dir(
        "mongo-contains",
        &[(".env", "DATABASE_URL=postgres://host/mongodb_replica_set")],
    );
    assert!(!ids(&dir).contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "mongo-http-scheme",
        &[(".env.local", "MONGO_URI=http://mongodb.example.com/app")],
    );
    assert!(!ids(&dir).contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // A comment mentioning mongodb must not match.
    let dir = in_dir(
        "mongo-comment",
        &[(".env", "# MONGODB_URI=mongodb://localhost/app")],
    );
    assert!(!ids(&dir).contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn does_not_detect_mongo_without_signature() {
    let dir = in_dir(
        "nomongo",
        &[(
            "docker-compose.yml",
            "services:\n  web:\n    image: nginx\n",
        )],
    );
    assert!(!ids(&dir).contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_sqlite_by_env_and_orm_config() {
    let dir = in_dir("sqlite", &[(".env", "DATABASE_URL=sqlite:///tmp/app.db")]);
    assert!(ids(&dir).contains(&"sqlite".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // .env.development is committable (not gitignored) and read by upone.
    let dir = in_dir(
        "sqlite-dev-env",
        &[(".env.development", "DATABASE_URL=sqlite:///./dev.db")],
    );
    assert!(ids(&dir).contains(&"sqlite".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "sqlite-prisma",
        &[(
            "prisma/schema.prisma",
            "datasource db { provider = \"sqlite\" }\n",
        )],
    );
    assert!(ids(&dir).contains(&"sqlite".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "sqlite-drizzle",
        &[(
            "drizzle.config.ts",
            "export default { dialect: \"sqlite\" }",
        )],
    );
    assert!(ids(&dir).contains(&"sqlite".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "sqlite-alembic",
        &[("alembic.ini", "sqlalchemy.url = sqlite:///./app.db")],
    );
    assert!(ids(&dir).contains(&"sqlite".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_mongoose_by_dependency() {
    let dir = in_dir(
        "mongoose",
        &[("package.json", r#"{"dependencies":{"mongoose":"^8.0.0"}}"#)],
    );
    assert!(ids(&dir).contains(&"mongoose".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // Present only in scripts must not match.
    let dir = in_dir(
        "no-mongoose",
        &[("package.json", r#"{"scripts":{"mongoose":"echo nope"}}"#)],
    );
    assert!(!ids(&dir).contains(&"mongoose".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_go() {
    let dir = in_dir("go", &[("go.mod", "module example.com/hello\n\ngo 1.24\n")]);
    assert!(ids(&dir).contains(&"go".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_python_pms_with_disambiguation() {
    // uv takes precedence over poetry and pip.
    let dir = in_dir(
        "py-all",
        &[
            ("uv.lock", "version = 1"),
            ("poetry.lock", "package = []"),
            ("requirements.txt", "requests\n"),
        ],
    );
    let found = ids(&dir);
    assert!(found.contains(&"uv".to_string()));
    assert!(!found.contains(&"poetry".to_string()));
    assert!(!found.contains(&"pip".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // poetry beats pip when uv is absent.
    let dir = in_dir(
        "py-poet",
        &[
            ("poetry.lock", "version = []"),
            ("requirements.txt", "flask\n"),
        ],
    );
    let found = ids(&dir);
    assert!(found.contains(&"poetry".to_string()));
    assert!(!found.contains(&"uv".to_string()));
    assert!(!found.contains(&"pip".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // requirements alone -> pip (never uv/poetry).
    let dir = in_dir("py-pip", &[("requirements.txt", "flask\n")]);
    let found = ids(&dir);
    assert!(found.contains(&"pip".to_string()));
    assert!(!found.contains(&"uv".to_string()));
    assert!(!found.contains(&"poetry".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // No python manifest -> nothing detected.
    let dir = in_dir("py-none", &[("README.md", "hello\n")]);
    let found = ids(&dir);
    assert!(!found.contains(&"uv".to_string()));
    assert!(!found.contains(&"poetry".to_string()));
    assert!(!found.contains(&"pip".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_yarn() {
    let dir = in_dir("yarn", &[("yarn.lock", "# THIS IS AN AUTOGENERATED FILE")]);
    assert!(ids(&dir).contains(&"yarn".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_ruby() {
    let dir = in_dir("ruby", &[("Gemfile", "source 'https://rubygems.org'\n")]);
    assert!(ids(&dir).contains(&"ruby".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir("ruby-lock", &[("Gemfile.lock", "GEM\n")]);
    assert!(ids(&dir).contains(&"ruby".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_php() {
    let dir = in_dir("php", &[("composer.json", "{\"name\":\"acme/app\"}")]);
    assert!(ids(&dir).contains(&"php".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir("php-lock", &[("composer.lock", "{\"packages\":[]}")]);
    assert!(ids(&dir).contains(&"php".to_string()));
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

#[test]
fn detects_typeorm() {
    let dir = in_dir(
        "typeorm",
        &[("package.json", r#"{"dependencies":{"typeorm":"^0.3.0"}}"#)],
    );
    assert!(ids(&dir).contains(&"typeorm".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // data-source file alone matches.
    let dir = in_dir(
        "typeorm-ds",
        &[
            (
                "data-source.ts",
                "export default new DataSource({ type: 'postgres' })",
            ),
            ("package.json", "{}"),
        ],
    );
    assert!(ids(&dir).contains(&"typeorm".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_sequelize() {
    let dir = in_dir(
        "sequelize",
        &[
            ("package.json", r#"{"dependencies":{"sequelize":"^6.0.0"}}"#),
            ("package-lock.json", "{}"),
        ],
    );
    assert!(ids(&dir).contains(&"sequelize".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // sequelize-cli layout alone matches, when the project is managed.
    let dir = in_dir(
        "sequelize-layout",
        &[
            ("config/config.json", "{\"development\":{}}"),
            ("migrations/1-init.js", "module.exports.up = () => {};"),
            ("package-lock.json", "{}"),
        ],
    );
    assert!(ids(&dir).contains(&"sequelize".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_knex() {
    let dir = in_dir(
        "knex",
        &[
            ("package.json", r#"{"dependencies":{"knex":"^3.0.0"}}"#),
            ("package-lock.json", "{}"),
        ],
    );
    assert!(ids(&dir).contains(&"knex".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // knexfile alone matches.
    let dir = in_dir("knex-file", &[("knexfile.js", "module.exports = {};")]);
    assert!(ids(&dir).contains(&"knex".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_ef_core() {
    let dir = in_dir(
        "ef",
        &[(
            "App.csproj",
            r#"<Project Sdk="Microsoft.NET.Sdk.Web">
  <ItemGroup>
    <PackageReference Include="Microsoft.EntityFrameworkCore.Sqlite" Version="9.0.0" />
  </ItemGroup>
</Project>"#,
        )],
    );
    assert!(ids(&dir).contains(&"ef-core".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // A plain csproj without EF must not match.
    let dir = in_dir(
        "no-ef",
        &[(
            "App.csproj",
            r#"<Project Sdk="Microsoft.NET.Sdk.Web"></Project>"#,
        )],
    );
    assert!(!ids(&dir).contains(&"ef-core".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_alembic() {
    let dir = in_dir(
        "alembic",
        &[("alembic.ini", "[alembic]\nscript_location = alembic\n")],
    );
    assert!(ids(&dir).contains(&"alembic".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_gorm_and_sqlalchemy() {
    let dir = in_dir(
        "gorm",
        &[(
            "go.mod",
            "module hello\n\ngo 1.24\n\nrequire (\n\tgorm.io/gorm v1.25.0\n)\n",
        )],
    );
    assert!(ids(&dir).contains(&"gorm".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir("sqlalchemy", &[("requirements.txt", "sqlalchemy==2.0.0\n")]);
    assert!(ids(&dir).contains(&"sqlalchemy".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // A manifest without the ORM must not match.
    let dir = in_dir(
        "no-sqlalchemy",
        &[("requirements.txt", "requests==2.0.0\n")],
    );
    assert!(!ids(&dir).contains(&"sqlalchemy".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_bun() {
    let dir = in_dir("bun", &[("bun.lock", "# bun lockfile")]);
    let found = ids(&dir);
    assert!(found.contains(&"bun".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir("bunb", &[("bun.lockb", "\0")]);
    assert!(ids(&dir).contains(&"bun".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_cargo() {
    let dir = in_dir("cargo", &[("Cargo.toml", "[package]\nname = \"demo\"\n")]);
    assert!(ids(&dir).contains(&"cargo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_biome_and_shadcn_and_turbo() {
    let dir = in_dir("biome", &[("biome.json", "{\"formatter\":{}}")]);
    assert!(ids(&dir).contains(&"biome".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir("shadcn", &[("components.json", "{\"style\":\"new-york\"}")]);
    assert!(ids(&dir).contains(&"shadcn".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir("turbo", &[("turbo.json", "{\"tasks\":{}}")]);
    assert!(ids(&dir).contains(&"turbo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_dependency_based_providers() {
    // better-auth by dependency.
    let pkg = manifest(&[("better-auth", "^1.0.0")]);
    let dir = in_dir("better-auth", &[("package.json", pkg.as_str())]);
    assert!(ids(&dir).contains(&"better-auth".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // @trpc/server by dependency.
    let pkg = manifest(&[("@trpc/server", "^11.0.0")]);
    let dir = in_dir("trpc", &[("package.json", pkg.as_str())]);
    assert!(ids(&dir).contains(&"trpc".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // next by both dependency and config file.
    let pkg = manifest(&[("next", "^15.0.0")]);
    let dir = in_dir("next-dep", &[("package.json", pkg.as_str())]);
    assert!(ids(&dir).contains(&"next".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "next-config",
        &[(
            "next.config.mjs",
            "const nextConfig = {};\nexport default nextConfig;",
        )],
    );
    assert!(ids(&dir).contains(&"next".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dependency_providers_ignore_non_dependency_matches() {
    // Only present in scripts/overrides must not match any of them.
    let dir = in_dir(
        "dep-neg",
        &[(
            "package.json",
            r#"{"scripts":{"next":"echo nope","trpc":"x"},"overrides":{"better-auth":"1.0.0"}}"#,
        )],
    );
    let found = ids(&dir);
    assert!(!found.contains(&"next".to_string()));
    assert!(!found.contains(&"trpc".to_string()));
    assert!(!found.contains(&"better-auth".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_redis_by_conf_signature() {
    let dir = in_dir("redis-conf", &[("redis.conf", "bind 127.0.0.1\n")]);
    assert!(ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "redis-sentinel",
        &[("redis/sentinel.conf", "sentinel monitor mymaster\n")],
    );
    assert!(ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn detects_redis_by_url() {
    // REDIS_URL with a redis scheme.
    let dir = in_dir(
        "redis-url",
        &[(".env", "REDIS_URL=redis://localhost:6379/0")],
    );
    assert!(ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // rediss:// (TLS) counts too, and DATABASE_URL may hold a redis URL.
    let dir = in_dir(
        "redis-srv",
        &[(".env.local", "DATABASE_URL=rediss://cache.example.com:6380")],
    );
    assert!(ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn does_not_detect_redis_when_value_merely_mentions_redis() {
    // Wrong scheme, non-redis key, or a comment must not match.
    let dir = in_dir(
        "redis-neg-scheme",
        &[(".env", "REDIS_URL=postgres://localhost/app")],
    );
    assert!(!ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "redis-neg-key",
        &[(".env.local", "CACHE_URL=redis://localhost:6379/0")],
    );
    assert!(!ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    let dir = in_dir(
        "redis-comment",
        &[(".env", "# REDIS_URL=redis://localhost:6379/0")],
    );
    assert!(!ids(&dir).contains(&"redis".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn database_url_scheme_discriminates_mysql_and_mongo() {
    // DATABASE_URL=postgres:// is postgres, never mysql or mongo.
    let dir = in_dir(
        "url-pg",
        &[(
            ".env",
            "DATABASE_URL=postgres://user:pass@localhost:5432/app",
        )],
    );
    let found = ids(&dir);
    assert!(found.contains(&"postgres".to_string()));
    assert!(!found.contains(&"mysql".to_string()));
    assert!(!found.contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // DATABASE_URL=mysql:// is mysql, never postgres or mongo.
    let dir = in_dir(
        "url-my",
        &[(".env", "DATABASE_URL=mysql://user:pass@localhost:3306/app")],
    );
    let found = ids(&dir);
    assert!(found.contains(&"mysql".to_string()));
    assert!(!found.contains(&"postgres".to_string()));
    assert!(!found.contains(&"mongo".to_string()));
    let _ = std::fs::remove_dir_all(&dir);

    // DATABASE_URL=mongodb:// is mongo, never mysql or postgres.
    let dir = in_dir(
        "url-mo",
        &[(".env", "DATABASE_URL=mongodb://localhost:27017/app")],
    );
    let found = ids(&dir);
    assert!(found.contains(&"mongo".to_string()));
    assert!(!found.contains(&"mysql".to_string()));
    assert!(!found.contains(&"postgres".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn optional_env_key_reports_warning_not_failure() {
    use upone_core::readiness::ReadinessStatus;
    use upone_core::{sweep, Context, Importance};
    use upone_providers::collect_readiness_checks;

    let dir = in_dir(
        "optional-env",
        &[(".env.example", "DATABASE_URL=\n# optional\nSTRIPE_KEY=\n")],
    );
    let ctx = Context { cwd: dir.clone() };
    let reg = build_registry();
    // No provider detections: only template requirements surface.
    let checks = collect_readiness_checks(&ctx, &[], &reg);
    assert_eq!(checks.len(), 2);

    let report = sweep(&ctx, &checks);

    let required = report
        .results
        .iter()
        .find(|r| r.id == "env-DATABASE_URL")
        .unwrap();
    assert_eq!(required.importance, Importance::Required);
    assert!(required.status.is_not_ready());

    let optional = report
        .results
        .iter()
        .find(|r| r.id == "env-STRIPE_KEY")
        .unwrap();
    assert_eq!(optional.importance, Importance::Optional);
    assert!(matches!(optional.status, ReadinessStatus::Warning { .. }));
    assert!(!report.is_ready());
    assert_eq!(report.warnings().len(), 1);
    assert_eq!(report.failures().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn provider_and_template_env_key_checks_are_deduplicated() {
    use upone_core::detect::detect;
    use upone_core::Context;
    use upone_providers::collect_readiness_checks;

    // Postgres is detected via DATABASE_URL (which adds an env-DATABASE_URL
    // check) and .env.example lists the same key — the template one must not
    // produce a second, conflicting check.
    let dir = in_dir(
        "dedup-env",
        &[
            (".env", "DATABASE_URL=postgres://localhost/app\n"),
            (".env.example", "DATABASE_URL=postgres://localhost/app\n"),
        ],
    );
    let reg = build_registry();
    let root_ctx = Context { cwd: dir.clone() };
    let detections = detect(&dir, &reg);
    let pkg_dets = detections
        .found
        .iter()
        .map(|d| (&root_ctx, d))
        .collect::<Vec<_>>();

    let checks = collect_readiness_checks(&root_ctx, &pkg_dets, &reg);
    let env_checks = checks.iter().filter(|c| c.id == "env-DATABASE_URL").count();
    assert_eq!(env_checks, 1);
    assert!(checks.iter().any(|c| c.id == "postgres-tcp"));

    let _ = std::fs::remove_dir_all(&dir);
}
