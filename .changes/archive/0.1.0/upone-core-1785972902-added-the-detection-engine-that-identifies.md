---
crate: upone-core
bump: minor
---

Added the detection engine that identifies a project's technology from file signatures.
Expose a pluggable `Provider` trait and a `Registry`, so new technologies are added without touching the core.
Added the `Planner`: a task DAG that orders tasks topologically and groups independent tasks into parallel levels.
Added the execution `Engine`, which runs task levels sequentially and independent tasks within a level concurrently.
Isolate task failures so a single failing task never aborts the rest of the plan.
Classify every task with a low / medium / high `Risk` so users can judge its side effects.
Produce a structured `Report` of every step, its status and any captured output.
