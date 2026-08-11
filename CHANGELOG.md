# Changelog

## 0.8.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.8.0 |
| upone-core | 0.5.0 |
| upone-providers | 0.7.1 |

### upone

- update upone-core 0.4.0 -> 0.5.0
- update upone-providers 0.7.0 -> 0.7.1

### upone-core

- Type task plan errors as PlanError and add RunError::Command for failed commands

### upone-providers

- Cache file reads and which() probes to cut redundant I/O during detection and planning
## 0.7.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.7.0 |
| upone-core | 0.4.0 |
| upone-providers | 0.7.0 |

### upone

- Delegate monorepo and workspace planning to upone-providers, slim down the CLI's own planning logic, and clean up the run report and tui output
- update upone-core 0.3.0 -> 0.4.0
- update upone-providers 0.6.0 -> 0.7.0

### upone-core

- Tighten the planner: reject duplicate task ids, missing dependencies and dependency cycles with clear errors, stamp every task with its working directory, support topological builds that ignore external tasks, and surface only real failures in the run report

### upone-providers

- Refactor providers around a shared command/migration helper layer and workspace planning (detect_workspace, plan_workspace, WorkspacePlan), detect redis through .env (REDIS_URL / postgres/redis DATABASE_URL schemes), drop the duplicated env-DATABASE_URL readiness check (now Optional/Warning), fix postgres readiness with a real protocol handshake so migrations no longer race the warm-up of a fresh container, and add a shared testkit plus provider tests
## 0.6.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.6.0 |
| upone-core | 0.3.0 |
| upone-providers | 0.6.0 |

### upone

- update upone-providers 0.5.0 -> 0.6.0

### upone-providers

- Add TypeORM, Sequelize, Knex, EF Core .NET, Alembic, GORM and SQLAlchemy providers
## 0.5.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.5.0 |
| upone-core | 0.3.0 |
| upone-providers | 0.5.0 |

### upone

- update upone-providers 0.4.0 -> 0.5.0

### upone-providers

- Added the `mysql` provider (detects `mysql`/`mariadb` in docker-compose or a `mysql://`/`mariadb://` `DATABASE_URL`, ensures the service responds on `localhost:3306`).
- Added the `mongo` provider (detects `mongo`/`mongodb` in docker-compose or a `mongodb://` URI via `MONGODB_URI`/`MONGO_URI`/`DATABASE_URL`, ensures the service responds on `localhost:27017`).
- Added the `sqlite` provider (detects a `sqlite://` `DATABASE_URL` or an ORM config targeting sqlite; creates the database file if missing — there is no server to start).
- Added the `mongoose` provider (recognizes the MongoDB ODM via the `mongoose` dependency; informational).
## 0.4.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.4.0 |
| upone-core | 0.3.0 |
| upone-providers | 0.4.0 |

### upone

- update upone-providers 0.3.0 -> 0.4.0

### upone-providers

- Added the `go` provider (detects `go.mod`, runs `go mod tidy` then `go build ./...`).
- Added Python providers `uv` (`uv.lock` -> `uv sync`), `poetry` (`poetry.lock` -> `poetry install`) and `pip` (requirements manifests installed into a project-local `.venv`), with lockfile-based precedence (uv wins over poetry, poetry over pip).
- Added the `yarn` provider (`yarn.lock` -> `yarn install`), picking `--immutable` for yarn berry and `--frozen-lockfile` for classic.
- Added the `ruby` provider (`Gemfile` -> `bundle install`) and the `php` provider (`composer.json`/`composer.lock` -> `composer install`).
## 0.3.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.3.0 |
| upone-core | 0.3.0 |
| upone-providers | 0.3.0 |

### upone

- Add `upone ready` subcommand for non-invasive environment readiness verification and integrate post-setup readiness sweep into `upone up`.
- update upone-core 0.2.0 -> 0.3.0
- update upone-providers 0.2.0 -> 0.3.0

### upone-core

- Add minimal environment readiness validation layer abstractions (ReadinessCheck, ReadinessStatus, ReadinessReport, sweep), .env* key resolver, and .env.example parser. Extend Provider trait with readiness_checks.

### upone-providers

- Implement readiness checks for postgres, redis, prisma, drizzle, cargo, and better-auth providers, and export collect_readiness_checks helper.
## 0.2.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.2.0 |
| upone-core | 0.2.0 |
| upone-providers | 0.2.0 |

### upone

- Recognize monorepos: detect at the workspace root and every package, run each task in the right package directory, and deduplicate repeated detections.
- update upone-core 0.1.0 -> 0.2.0
- update upone-providers 0.1.1 -> 0.2.0

### upone-core

- Tasks now carry their working directory, so a plan can mix root and package-level tasks (a monorepo `drizzle-kit generate` runs inside its package).
- Add `Planner::build_allow_external` so a workspace package can depend on the root install task before plans are merged.

### upone-providers

- Detect across bun/npm/pnpm workspaces: discover package directories and scan them for drizzle, postgres, prisma and the rest.
- New detection-only providers: turbo, biome, shadcn, next, trpc and better-auth.
- Drizzle and prisma check tasks now wait for the package-manager install, fixing a race that failed when node_modules was missing.
## 0.1.1

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.1.1 |
| upone-core | 0.1.0 |
| upone-providers | 0.1.1 |

### upone

- Exit with a non-zero status when any task in the plan fails, so scripts and CI pipelines can detect a failed run.
- update upone-providers 0.1.0 -> 0.1.1

### upone-providers

- Ensure postgres and redis never start a second, redundant `docker compose` invocation: when a compose file defines them they depend on the docker provider's compose-up task and only verify the service responds, removing a race between concurrent compose runs.
- Report a clear, actionable error when postgres or redis are detected without a compose service to start them, instead of a broken `docker compose up`.
- Show the tail of a failing command's output as the task error message, instead of a truncated first line.
- Verify postgres and redis on the actual host port a compose file publishes for them, instead of assuming 5432/6379, so projects mapping alternative ports are checked correctly.
## 0.1.0

Crate versions in this release:

| Crate | Version |
| --- | --- |
| upone | 0.1.0 |
| upone-core | 0.1.0 |
| upone-providers | 0.1.0 |

### upone

- Introduced the interactive `up` command (clap) that prepares a project for development.
- Preview the detected providers and the full generated plan before running anything.
- Added `--dry-run` to print the plan and exit without executing any task.
- Added `--yes` to skip the confirmation prompt for scripted or automated runs.
- Render a live terminal UI (ratatui/crossterm) with per-task status, a spinner and risk labels while tasks run.
- Fall back to a plain, non-interactive summary when stdin/stdout are not a terminal (pipes and CI).
- update upone-core 0.0.0 -> 0.1.0
- update upone-providers 0.0.0 -> 0.1.0

### upone-core

- Added the detection engine that identifies a project's technology from file signatures.
- Expose a pluggable `Provider` trait and a `Registry`, so new technologies are added without touching the core.
- Added the `Planner`: a task DAG that orders tasks topologically and groups independent tasks into parallel levels.
- Added the execution `Engine`, which runs task levels sequentially and independent tasks within a level concurrently.
- Isolate task failures so a single failing task never aborts the rest of the plan.
- Classify every task with a low / medium / high `Risk` so users can judge its side effects.
- Produce a structured `Report` of every step, its status and any captured output.

### upone-providers

- Added JavaScript package-manager providers for bun, npm and pnpm that check the binary and install dependencies.
- Added the cargo provider for Rust projects.
- Added the docker provider that brings up compose services in the background.
- Added the prisma and drizzle providers for ORM client generation.
- Added the redis and postgres providers that ensure backing services are running.
