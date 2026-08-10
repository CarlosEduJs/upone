---
crate: upone-core
bump: minor
---

Tighten the planner: reject duplicate task ids, missing dependencies and dependency cycles with clear errors, stamp every task with its working directory, support topological builds that ignore external tasks, and surface only real failures in the run report
