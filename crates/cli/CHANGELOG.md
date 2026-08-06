# Changelog

## 0.1.1

- Exit with a non-zero status when any task in the plan fails, so scripts and CI pipelines can detect a failed run.
- update upone-providers 0.1.0 -> 0.1.1
## 0.1.0

- Introduced the interactive `up` command (clap) that prepares a project for development.
- Preview the detected providers and the full generated plan before running anything.
- Added `--dry-run` to print the plan and exit without executing any task.
- Added `--yes` to skip the confirmation prompt for scripted or automated runs.
- Render a live terminal UI (ratatui/crossterm) with per-task status, a spinner and risk labels while tasks run.
- Fall back to a plain, non-interactive summary when stdin/stdout are not a terminal (pipes and CI).
- update upone-core 0.0.0 -> 0.1.0
- update upone-providers 0.0.0 -> 0.1.0
