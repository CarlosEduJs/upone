# Changesets

This directory holds **changeset notes**: one small markdown file per change, written
alongside the PR that introduces it. When notes are present, CI turns them into a
version-bump PR, per-crate changelog entries and a GitHub release.

## Format

Every note is a markdown file with TOML-style frontmatter:

````md
---
crate: upone
bump: minor
---

Added `--dry-run` and `--yes` flags.
````

- **`crate`** — which crate changed. Accepts the package name or an alias:

  | Package | Aliases |
  | --- | --- |
  | `upone` | `cli` |
  | `upone-core` | `core` |
  | `upone-providers` | `providers` |

- **`bump`** — semver severity: `patch`, `minor` or `major`.

- **Body** — one or more lines describing the change. Each line becomes a bullet in
  the changelog and the release notes.

## Scaffolding

The fastest way to create a note:

```bash
cargo xtask new changeset upone --bump minor --summary "Added --dry-run and --yes flags"
```

Or just drop a file by hand in `.changes/` (any `.md` file except `README.md`).

## What happens

1. Notes land on `main` (merged with their PRs).
2. `version.yml` sees them and runs `cargo xtask version`, which:
   - computes the new version of each affected crate (highest `bump` wins);
   - also bumps `upone` (the shipped binary) whenever `upone-core`/`upone-providers` bump;
   - prepends entries to each crate's `CHANGELOG.md` and to the root `CHANGELOG.md`;
   - moves the consumed notes into `.changes/archive/<version>/`;
   - opens a `release/vX.Y.Z` PR whose body shows the per-crate tables.
3. Once that PR merges, the release is dispatched to `cargo-dist`, which builds the
   binaries, creates the GitHub release and — via `post-release.yml` — its body is
   filled with these notes.