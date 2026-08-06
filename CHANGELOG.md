# Changelog

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
