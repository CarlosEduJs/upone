---
crate: upone-providers
bump: minor
---

Added the `go` provider (detects `go.mod`, runs `go mod tidy` then `go build ./...`).
Added Python providers `uv` (`uv.lock` -> `uv sync`), `poetry` (`poetry.lock` -> `poetry install`) and `pip` (requirements manifests installed into a project-local `.venv`), with lockfile-based precedence (uv wins over poetry, poetry over pip).
Added the `yarn` provider (`yarn.lock` -> `yarn install`), picking `--immutable` for yarn berry and `--frozen-lockfile` for classic.
Added the `ruby` provider (`Gemfile` -> `bundle install`) and the `php` provider (`composer.json`/`composer.lock` -> `composer install`).