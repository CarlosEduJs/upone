# Changelog

## 0.6.0

- Add TypeORM, Sequelize, Knex, EF Core .NET, Alembic, GORM and SQLAlchemy providers
## 0.5.0

- Added the `mysql` provider (detects `mysql`/`mariadb` in docker-compose or a `mysql://`/`mariadb://` `DATABASE_URL`, ensures the service responds on `localhost:3306`).
- Added the `mongo` provider (detects `mongo`/`mongodb` in docker-compose or a `mongodb://` URI via `MONGODB_URI`/`MONGO_URI`/`DATABASE_URL`, ensures the service responds on `localhost:27017`).
- Added the `sqlite` provider (detects a `sqlite://` `DATABASE_URL` or an ORM config targeting sqlite; creates the database file if missing — there is no server to start).
- Added the `mongoose` provider (recognizes the MongoDB ODM via the `mongoose` dependency; informational).
## 0.4.0

- Added the `go` provider (detects `go.mod`, runs `go mod tidy` then `go build ./...`).
- Added Python providers `uv` (`uv.lock` -> `uv sync`), `poetry` (`poetry.lock` -> `poetry install`) and `pip` (requirements manifests installed into a project-local `.venv`), with lockfile-based precedence (uv wins over poetry, poetry over pip).
- Added the `yarn` provider (`yarn.lock` -> `yarn install`), picking `--immutable` for yarn berry and `--frozen-lockfile` for classic.
- Added the `ruby` provider (`Gemfile` -> `bundle install`) and the `php` provider (`composer.json`/`composer.lock` -> `composer install`).
## 0.3.0

- Implement readiness checks for postgres, redis, prisma, drizzle, cargo, and better-auth providers, and export collect_readiness_checks helper.
## 0.2.0

- Detect across bun/npm/pnpm workspaces: discover package directories and scan them for drizzle, postgres, prisma and the rest.
- New detection-only providers: turbo, biome, shadcn, next, trpc and better-auth.
- Drizzle and prisma check tasks now wait for the package-manager install, fixing a race that failed when node_modules was missing.
## 0.1.1

- Ensure postgres and redis never start a second, redundant `docker compose` invocation: when a compose file defines them they depend on the docker provider's compose-up task and only verify the service responds, removing a race between concurrent compose runs.
- Report a clear, actionable error when postgres or redis are detected without a compose service to start them, instead of a broken `docker compose up`.
- Show the tail of a failing command's output as the task error message, instead of a truncated first line.
- Verify postgres and redis on the actual host port a compose file publishes for them, instead of assuming 5432/6379, so projects mapping alternative ports are checked correctly.
## 0.1.0

- Added JavaScript package-manager providers for bun, npm and pnpm that check the binary and install dependencies.
- Added the cargo provider for Rust projects.
- Added the docker provider that brings up compose services in the background.
- Added the prisma and drizzle providers for ORM client generation.
- Added the redis and postgres providers that ensure backing services are running.
