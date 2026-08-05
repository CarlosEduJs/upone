# upone

Prepare your development environment with a single command.

upone looks at your project, works out which technologies it uses, and runs
everything needed to get it ready to develop — install dependencies, build,
start the databases, and generate the tools your stack needs.

No configuration files to write. No per-tool playbook to memorize. Just
`upone up`.

## What it does

When you run `upone up`, it:

1. **Detects** the technologies in your project** by inspecting well-known file signatures (for example `Cargo.toml`, `pnpm-lock.yaml` or `docker-compose.yml`) — no manual setup required.
2. **Builds a plan** of the tasks needed to prepare the project, laid out as a dependency graph: things that depend on other tasks run in the right order, and independent tasks run in parallel.
3. **Runs the plan**, streaming live progress in an interactive terminal UI, and gives you a clear pass/fail summary when it's done.

You stay in control: a preview shows the plan and its risk level before anything runs, `--dry-run` shows the plan without executing, and `--yes` skips the confirmation when you just want it done.

## What it supports today

| Technology | Detection signature | What it does |
| --- | --- | --- |
| **cargo** | `Cargo.toml` | Checks `cargo is installed and runs `cargo build` |
| **npm** | `package-lock.json` | Installs dependencies with `npm install --no-audit --no-fund` |
| **pnpm** | `pnpm-lock.yaml` | Installs dependencies with `pnpm install --frozen-lockfile` |
| **bun** | `bun.lock` / `bun.lockb` | Installs dependencies with `bun install` |
| **docker** | `docker-compose.yml` / `compose.yml` | Brings up the defined services with `docker compose up -d` |
| **PostgreSQL** | `docker-compose` `postgres` / `DATABASE_URL` | Makes sure postgres is responding on `localhost:5432`, starting it if needed |
| **Redis** | `docker-compose` `redis` / `redis.conf` | Makes sure redis is responding on `localhost:6379`, starting it if needed |
| **Prisma** | `prisma/schema.prisma` | Generates the Prisma client with `npx prisma generate` |
| **Drizzle** | `drizzle.config.*` | Generates migrations with `npx drizzle-kit generate` |

A project can use several providers at once — upone detects them all, orders their
tasks, and runs something like `pnpm install` → `prisma generate` back-to-back,
without you having to remember the sequence.

## What it improves

Developers spend the first minutes of every session getting the environment back up:
installing dependencies that moved, starting databases that stopped, regenerating
clients after a schema change. upone turns that repeatable work into one command.

- **One command instead of a manual checklist** — the plan is derived from the code, so there's nothing to remember or keep in sync.
- **Zero configuration** detection — the stack is inferred from lockfiles and config files, not a hand-maintained manifest.
- **Safe to repeat.** Database service tasks only start a service when it's not already responding, so running it twice doesn't fail on "already running".
- **Right order, automatically.** Dependencies between steps (install before generate, machine-check before build) are encoded in the plan, so ordering bugs disappear.
- **Parallel where it can be.** Independent tasks run in the same pass instead of serially.
- **Safe by default.** A plan preview with risk levels, an interactive confirmation, and a dry-run — you approve every change before it touches your machine.

## Friction it removes

- Remembering which package manager or build command a project uses.
- Failing an install because a lockfile got out of sync (`--frozen-lockfile` is
  enforced for reproducible installs).
- Starting databases with the wrong/different steps per-person environment and then dealing with "port already in use".
- Regenerating ORM clients by hand after every schema edit.
- Bringing a brand-new teammate's machine from "cloned" to "developing" with zero hand-holding.

## Usage

```bash
# from inside a project directory
upone up            # detect, preview, confirm, run
upone up --dry-run  # only show the plan
upone up --yes      # run without asking for confirmation
```

Run the CLI and it always detects the current directory — no flags to configure the stack.

## Examples

Ready to run example projects live under `examples/`. Validate the flow end-to-end with:

```bash
scripts/verify-examples.sh           # dry-run each example
scripts/verify-examples.sh --exec    # also run --yes in each
```

## License

[MIT](./LICENSE)