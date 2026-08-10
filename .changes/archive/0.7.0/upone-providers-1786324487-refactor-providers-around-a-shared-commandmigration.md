---
crate: upone-providers
bump: minor
---

Refactor providers around a shared command/migration helper layer and workspace planning (detect_workspace, plan_workspace, WorkspacePlan), detect redis through .env (REDIS_URL / postgres/redis DATABASE_URL schemes), drop the duplicated env-DATABASE_URL readiness check (now Optional/Warning), fix postgres readiness with a real protocol handshake so migrations no longer race the warm-up of a fresh container, and add a shared testkit plus provider tests
