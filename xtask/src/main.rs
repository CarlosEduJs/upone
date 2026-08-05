//! `xtask` — release tooling for the upone workspace.
//!
//! Subcommands:
//! - `new` — scaffold a changeset note under `.changes/`
//! - `version` — aggregate `.changes/` notes, bump crate versions, write changelogs
//! - `update-release-body` — render a GitHub release body (optionally publish it)
//! - `pending-release-tag` — print the tag to release, if a new release is pending

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

mod changelog;
mod changes;
mod cx;
mod pending_tag;
mod release_body;
mod version;

#[derive(Parser)]
#[command(name = "xtask", about = "upone release tooling")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scaffold a new changeset note under `.changes/`.
    New(NewArgs),
    /// Aggregate `.changes/` notes, bump versions, write changelogs and archive consumed notes.
    Version(VersionArgs),
    /// Render a GitHub release body for a tag; optionally publish it via `gh`.
    UpdateReleaseBody(UpdateBodyArgs),
    /// Print the version to release if a new release is pending, otherwise nothing.
    PendingReleaseTag,
}

#[derive(clap::Args)]
struct NewArgs {
    /// Crate to record the change for: upone | upone-core | upone-providers (aliases accepted).
    crate_name: String,
    /// Bump type: patch, minor or major.
    #[arg(long, default_value = "patch")]
    bump: changes::Bump,
    /// Human-readable summary of the change (becomes the note body).
    #[arg(long)]
    summary: String,
}

#[derive(clap::Args)]
struct VersionArgs {
    /// Only print what would change, without touching any files.
    #[arg(long)]
    dry_run: bool,
}

#[derive(clap::Args)]
struct UpdateBodyArgs {
    /// Version to render, e.g. `0.2.0` (leading `v` is tolerated).
    tag: String,
    /// Also run `gh release edit` to publish the rendered body.
    #[arg(long)]
    publish: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::New(a) => run_new(a),
        Cmd::Version(a) => version::run(a),
        Cmd::UpdateReleaseBody(a) => release_body::run_update(a),
        Cmd::PendingReleaseTag => pending_tag::run(),
    }
}

/// Finds the workspace root (dir with Cargo.toml + .git).
pub fn workspace_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join("Cargo.toml").is_file() && dir.join(".git").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(anyhow!("could not find the workspace root"));
        }
    }
}

fn run_new(args: NewArgs) -> Result<()> {
    let root = workspace_root()?;
    changes::new_note(&root, &args.crate_name, args.bump, &args.summary)
}
