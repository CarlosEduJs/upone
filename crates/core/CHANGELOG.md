# Changelog

## 0.3.0

- Add minimal environment readiness validation layer abstractions (ReadinessCheck, ReadinessStatus, ReadinessReport, sweep), .env* key resolver, and .env.example parser. Extend Provider trait with readiness_checks.
## 0.2.0

- Tasks now carry their working directory, so a plan can mix root and package-level tasks (a monorepo `drizzle-kit generate` runs inside its package).
- Add `Planner::build_allow_external` so a workspace package can depend on the root install task before plans are merged.
## 0.1.0

- Added the detection engine that identifies a project's technology from file signatures.
- Expose a pluggable `Provider` trait and a `Registry`, so new technologies are added without touching the core.
- Added the `Planner`: a task DAG that orders tasks topologically and groups independent tasks into parallel levels.
- Added the execution `Engine`, which runs task levels sequentially and independent tasks within a level concurrently.
- Isolate task failures so a single failing task never aborts the rest of the plan.
- Classify every task with a low / medium / high `Risk` so users can judge its side effects.
- Produce a structured `Report` of every step, its status and any captured output.
