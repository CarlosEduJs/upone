# AGENTS.md — upone

Guidance for AI-assisted and human contributors working in this repository. Read
this before touching code. Commands below are the real ones used by CI and the
repo's own scripts — copy them verbatim.

## 1. Project overview

`upone` is a Rust CLI that prepares a development environment with one command:
`upone up`. It inspects a project's file signatures (`Cargo.toml`,
`pnpm-lock.yaml`, `docker-compose.yml`, `prisma/schema.prisma`, `.env.example`,
...), builds a dependency-ordered task DAG, and runs it with a live ratatui
terminal UI (or plain text when piped). `upone ready` is the read-only
counterpart: it runs the same detection, executes lightweight readiness checks,
and exits non-zero when a *required* check is not ready.

Architecture is a Cargo workspace of three crates plus release tooling:

| Crate (dir) | Role | Purpose |
| --- | --- | --- |
| `upone-core` (`crates/core`) | engine | **Technology-agnostic**. `Provider` trait + `Registry`, `Planner`/`Plan`/`Task` DAG, execution `Engine`, `Report`, readiness layer (`ReadinessCheck`, `sweep`, env helpers). Only dependency: `thiserror`. |
| `upone-providers` (`crates/providers`) | providers | Bundled technology providers (one file per tech) + shared `cmd.rs` command helpers + `workspace.rs` monorepo discovery/planning. Reuses nothing from `upone-core` beyond its APIs. Dependencies: `upone-core`, `serde_json`. |
| `upone` (`crates/cli`) | binary | clap CLI (`up`, `ready`), the ratatui/crossterm TUI (`tui.rs`), and plain-text reports (`report.rs`). |
| `xtask` (`xtask/`) | tooling | Release tooling: changeset scaffold (`new`), version aggregation (`version`), release body (`update-release-body`), pending tag (`pending-release-tag`). |

A new technology to support is implemented as a **provider in `upone-providers`**
and registered in `build_registry()` in `crates/providers/src/lib.rs`. Technology
knowledge must never leak into `upone-core`.

## 2. Environment setup

Requires a stable Rust toolchain (CI uses `dtolnay/rust-toolchain@stable`;
local toolchain 1.96). No env vars are needed to build the tool itself.

```bash
# Clone and build the workspace
git clone git@github.com:CarlosEduJs/upone.git
cd upone
cargo build --workspace --locked

# Run the CLI directly from source (against whatever cwd you're in)
cargo run -p upone -- up                 # detect + preview + run
cargo run -p upone -- up --dry-run       # show the plan only
cargo run -p upone -- up --yes           # skip confirmation
cargo run -p upone -- ready              # read-only readiness report

# Install to PATH from the repo root
cargo install --path crates/cli --locked
```

`.cargo/config.toml` aliases `cargo xtask <cmd>` to `cargo run -p xtask -- <cmd>`,
so release commands are typed as `cargo xtask ...`.

## 3. Essential commands

All of these are the exact CI gates — run them before pushing:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked

# Example fixtures: dry-run every example plan (no network/docker)
./scripts/verify-examples.sh
# Full integration: actually installs deps / starts docker in each example
./scripts/verify-examples.sh --exec
```

Release tooling:

```bash
cargo xtask new upone --bump minor --summary "Added a new provider"   # changeset note
cargo xtask new upone-core --bump patch --summary "Fix planner cycle error"
cargo xtask version             # aggregate .changes/ notes, bump versions, write changelogs
cargo xtask version --dry-run   # show what it would do without writing
```

## 4. Code conventions

**Lints (enforced, from `[workspace.lints]` in `Cargo.toml`):** `unsafe_code =
forbid`; clippy `all`, `pedantic`, `nursery`, `cargo` warn by default; and
specifically `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`,
`print_stdout`, `print_stderr`, `dbg_macro` all `warn`. In production code use
`anyhow`/`thiserror` and pattern matching instead of unwraps. In test modules
opt out explicitly with `#![allow(clippy::unwrap_used)]` (see
`crates/core/src/plan.rs` tests, `crates/providers/src/workspace.rs`,
`crates/providers/tests/*.rs`).

**Provider shape** — every provider is `pub struct X; impl Provider for X` with:
- `id()` — stable provider id (`"bun"`, `"postgres"`, `"ef-core"`).
- `signatures()` — file signatures; return `&[]` for content-only detection and
  override `detect()` instead (see `crates/providers/src/postgres.rs`).
- `plan(&self, ctx, planner)` — adds `Task`s.
- `readiness_checks(&self, ctx)` — optional; default empty.

**Task shape** — `Task::new(id, label, description)` is a builder:
`.risk(Risk)` `.depends_on([ids])` `.run(fn)`. Conventions:
- ids are `kebab` and unique; typical patterns: `{pm}-check` / `{pm}-install`,
  `{provider}-up`, `{provider}-generate`, `{provider}-migrate`, `{provider}-venv`,
  `{provider}-ensure`. Changing an id is a breaking change to plan tests and
  to monorepo namespacing.
- `Risk::Low` = no side effects; `Risk::Medium` = installs / network; `Risk::High`
  = mutates global state (databases, docker, migrations).

**Reuse the shared helpers** in `crates/providers/src/cmd.rs` instead of
hand-rolling command execution, port probing or manifest parsing:
`spawn_cmd`, `which`/`which_probe`/`check_binary`, `tcp_reachable`,
`compose_host_port`, `js_install_task`, `python_install_task`,
`migration_db_dep`, `add_migration_plan`, `env_key_check`, `node_modules_check`,
`package_has_dependency`, `node_modules_present`, `files_contain`, `any_exists`.

**Package-manager providers** reuse `JsPm` + `add_install_plan`
(`crates/providers/src/js.rs`) — see `bun.rs` for the minimal pattern.

**Providers shell out to real CLIs; do not vendor libraries.** `upone-providers`
deliberately depends only on `upone-core` and `serde_json`. Detection uses
lockfiles/config files read as strings or JSON, not third-party parsers.

**Readiness checks are non-invasive** — they inspect state (port open, file
exists, env var set) — they never "run" a command. Follow the pattern in
`crates/core/src/readiness.rs` and `crates/providers/src/cargo.rs`.

## 5. Directory structure

```
crates/core/src/        detection engine, Plan/Planner/Task, Engine, readiness, env parsers
crates/core/tests/      integration tests (core.rs)
crates/providers/src/   one file per provider + cmd.rs + js.rs + python.rs + workspace.rs + testkit.rs
crates/providers/tests/ detection tests (providers.rs), plan tests (plan.rs)
crates/cli/src/         main.rs (CLI), tui.rs (ratatui), report.rs (plain text)
xtask/src/              changes.rs, version.rs, changelog.rs, release_body.rs, cx.rs, pending_tag.rs
examples/<name>/        fixture projects used by scripts/verify-examples.sh (see below)
.changes/               changeset notes (one .md per change); archive/<version>/ holds consumed notes
scripts/                verify-examples.sh
.github/workflows/      ci.yml, version.yml, dispatch-release.yml, release.yml, post-release.yml
dist-workspace.toml     cargo-dist release config
```

`examples/` fixtures: one directory per scenario. Directories starting with `_`
are shared helpers, skipped by the verifier. `scripts/verify-examples.sh` maps
each example name to expected plan tokens and, on `--exec`, to cleanup commands.
Add a new example there (and to the `case` in `verify-examples.sh`) when you add
providers.

## 6. Testing rules

- **Unit tests** live inline in module files (`#[cfg(test)] mod tests`); see
  `core/src/plan.rs`, `core/src/readiness.rs`, `providers/src/workspace.rs`,
  `providers/src/cmd.rs`.
- **Integration tests** live in `crates/<name>/tests/`:
  - `providers/tests/providers.rs` — detection: write fixture files, run
    `detect()`, assert which provider ids are found (and negative cases).
  - `providers/tests/plan.rs` — plan shape: assert task ids, `deps`, `risk` for
    each provider and the install/db edges wired by `add_migration_plan`.
- **Fixtures are materialized in temp dirs**, never in the repo. Both test files
  and `cmd.rs` replicate an `in_dir()`/`temp_dir()` helper that embeds the process
  id so parallel tests don't collide, writes files, and removes the dir after.
  In `providers/src`, prefer `crate::testkit::temp_dir` over a local copy.
- Run the full gate before committing:
  `cargo test --workspace --locked` (plus fmt/clippy above).
- For a **new provider**: add detection tests to `providers/tests/providers.rs`
  and plan tests to `providers/tests/plan.rs`, mirroring the existing cases.
- The example `--exec` pass installs/creates artifacts (`node_modules`, `.venv`,
  `Gemfile.lock`, migrations, sqlite files); `verify-examples.sh` cleans them and
  the `.gitignore` covers the rest. Don't commit fixture outputs.

## 7. Contribution flow

- **Branch naming**: repo history uses `feat/<thing>` (e.g. `feat/more-db`,
  `feat/monorepo`), `fix-lint`, `add-<feature>`, `ci-validation-gate`, and
  `release/vX.Y.Z` (auto-created by `version.yml`).
- **Commit messages**: terse, lowercase for wip commits (`refactor`, `fix issues`,
  `remove dead code`), imperative summary for feature commits
  (`Add TypeORM, Sequelize, Knex ... providers (#16)`), and `chore: release vX.Y.Z`
  for release bumps. Matching repo style beats pretending there are strict rules.
- **Every user-facing/provider change ships a changeset note** under `.changes/`
  (YAML-frontmatter markdown with `crate:` and `bump:`). Scaffold with:
  ```bash
  cargo xtask new upone --bump minor --summary "One-line summary of the change"
  ```
  `crate` accepts aliases: `cli`→upone, `core`→upone-core, `providers`→upone-providers.
  When `upone-core` or `upone-providers` bumps, the `upone` binary bumps too
  (handled by `xtask version`).
- **CI gate** (`ci.yml`) runs on `main` and every PR: fmt, clippy `-D warnings`,
  `cargo test --workspace --locked`, and example dry-runs. The `--exec` integration
  pass runs on `main` and on PRs touching `crates/**`, `examples/**`,
  `scripts/**`, `Cargo.toml`, `Cargo.lock`.
- **Releases**: `version.yml` sees merged notes and runs `cargo xtask version`,
  which bumps manifests, prepends changelog entries, archives consumed notes, and
  opens a `chore: release vX.Y.Z` PR. After it merges, `dispatch-release.yml`
  triggers cargo-dist, which builds/publishes; `post-release.yml` fills the release
  body. Never hand-edit versions or changelogs — treat this pipeline as the owner.

## 8. What NOT to do

- **Do not add technology knowledge to `upone-core`.** New techs are new provider
  files in `upone-providers`. The core's `Provider` trait exists precisely to keep
  it tech-agnostic.
- **Do not start a second `docker compose up`.** The `docker` provider's task is
  the single owner that starts compose services; `postgres`/`redis`/`mysql`/`mongo`
  depend on `docker-up` and only verify the service responds (see the modules'
  top-of-file doc comments).
- **Do not assume fixed ports.** Read the host port a compose service publishes
  with `compose_host_port` instead of hard-coding `5432`/`6379`/`3306`/`27017`
  (postgres/redis fix commit history shows why this matters).
- **Do not grep substrings in `.env` files naively for connection URIs.** Scheme
  matters: `DATABASE_URL=postgres://` must only detect postgres — precedent in
  `database_url_scheme_discriminates_mysql_and_mongo` covers these edge cases.
- **Do not break task-id uniqueness** — the planner rejects duplicate ids, and
  `workspace.rs` namespaces per-package ids with `dir_slug` (`packages__db` vs
  `packages_db`). Changing id schemas breaks `plan.rs`/`workspace.rs` tests and
  monorepo ordering.
- **Do not run arbitrary commands from readiness checks.** Readiness is defined as
  inspecting state; changing that contract breaks the module's documented purpose.
- **Do not hand-edit `CHANGELOG.md`, crate versions, or `Cargo.lock`** for release
  purposes — `cargo xtask version` owns them.
- **Do not add a dependency to `upone-providers`** to parse something that is read
  as a string or JSON; keep the dependency surface minimal (`upone-core`,
  `serde_json` today).
- **Keep cargo invocations `--locked`** in scripts/CI (`verify-examples.sh`,
  workflows) so lockfile drift is caught instead of silently regenerated.
- **Don't convert dry-run-only example fixtures to `--exec`** when they have no
  local database service to migrate against (dotnet-ef, alembic, gorm,
  sqlalchemy) — see the comment in `verify-examples.sh`.

## 9. Security and sensitive data

- upone reads **other projects'** `.env*` / `DATABASE_URL` values at runtime to
  detect the stack. It must **never print the values** — only boolean/presence
  results ("found", "responding on localhost:5432"). Do not add logging that
  echoes env values, connection strings or keys.
- The `.env.example` / `.env.template` parser only extracts *key names* and
  whether a `# optional` comment precedes them — never secrets.
- Tests and examples use placeholder credentials (`user:pass`,
  `sk_test_xxx`, `change-me`) and localhost URIs. Keep it that way.
- CI secrets are referenced only in workflow files via `${{ secrets.* }}`
  (e.g. `UPONE_RELEASE_PAT` in `post-release.yml`). Never hard-code or persist
  tokens, and never echo `secrets.*` into logs.
- `.gitignore` deliberately keeps fixture lockfiles and artifacts out of the repo;
  keep `.env`/`.env.local`-style real credentials out of commits.