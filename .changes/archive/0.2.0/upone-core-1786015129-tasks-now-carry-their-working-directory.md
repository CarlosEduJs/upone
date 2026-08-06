---
crate: upone-core
bump: minor
---

Tasks now carry their working directory, so a plan can mix root and package-level tasks (a monorepo `drizzle-kit generate` runs inside its package).
Add `Planner::build_allow_external` so a workspace package can depend on the root install task before plans are merged.
