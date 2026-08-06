---
crate: upone-providers
bump: minor
---

Detect across bun/npm/pnpm workspaces: discover package directories and scan them for drizzle, postgres, prisma and the rest.
New detection-only providers: turbo, biome, shadcn, next, trpc and better-auth.
Drizzle and prisma check tasks now wait for the package-manager install, fixing a race that failed when node_modules was missing.
