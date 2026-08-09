//! `ef-core` provider: detects .NET projects that reference Entity Framework
//! Core and prepares them with `dotnet restore` + `dotnet ef database update`.
//!
//! This is upone's first .NET provider. Detection walks the project root for a
//! `<Project>` that references `Microsoft.EntityFrameworkCore` (or the
//! `.Design` companion), so EF migrations can be applied.

use std::path::Path;

use upone_core::detect::Provider;
use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::{Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::{Context, Risk};

use crate::cmd::{migration_db_dep, spawn_cmd, which};

pub struct EfCore;

impl Provider for EfCore {
    fn id(&self) -> &'static str {
        "ef-core"
    }

    fn signatures(&self) -> &'static [&'static str] {
        &[]
    }

    fn detect(&self, cwd: &Path) -> Option<upone_core::Detection> {
        find_csproj_with_ef(cwd).map(|rel| upone_core::Detection {
            provider: self.id(),
            signature: rel,
            reason: "Entity Framework Core detected in a C# project".into(),
        })
    }

    fn plan(&self, ctx: &Context, planner: &mut Planner<'_>) {
        let check = Task::new(
            "ef-check",
            "check dotnet installed",
            "checks that the dotnet SDK is on PATH",
        )
        .risk(Risk::Low)
        .run(check_dotnet);

        let restore = Task::new(
            "ef-restore",
            "dotnet restore",
            "restores the project's NuGet packages and the dotnet-ef tool (safe to repeat)",
        )
        .risk(Risk::Medium)
        .depends_on(["ef-check"])
        .run(dotnet_restore);

        let mut update = Task::new(
            "ef-update",
            "dotnet ef database update",
            "applies pending EF Core migrations to the configured database (safe to repeat)",
        )
        .risk(Risk::High)
        .depends_on(["ef-check", "ef-restore"])
        .run(ef_database_update);

        if let Some(db) = migration_db_dep(&ctx.cwd) {
            update = update.depends_on(["ef-check", db]);
        }

        planner.add(check);
        planner.add(restore);
        planner.add(update);
    }

    fn readiness_checks(&self, _ctx: &Context) -> Vec<ReadinessCheck> {
        vec![ReadinessCheck::new(
            "ef-dotnet",
            "dotnet on PATH",
            "dotnet SDK is available for the EF Core tooling",
            Importance::Required,
            |_ctx| {
                if which("dotnet") {
                    ReadinessStatus::Ready("dotnet found".into())
                } else {
                    ReadinessStatus::NotReady {
                        reason: "dotnet not found on PATH".into(),
                        remedy: "Install the .NET SDK via https://dotnet.microsoft.com/download"
                            .into(),
                    }
                }
            },
        )]
    }
}

/// Finds a `.csproj` (relative to `cwd`) whose content references an EF Core
/// package. Recursive, but bounded to the project — never escapes `cwd` or
/// descends into `node_modules`/`.git`/`obj`/`bin`.
fn find_csproj_with_ef(cwd: &Path) -> Option<String> {
    find_csproj_with_ef_in(cwd, cwd, 0)
}

const MAX_DEPTH: usize = 6;

fn find_csproj_with_ef_in(cwd: &Path, dir: &Path, depth: usize) -> Option<String> {
    if depth > MAX_DEPTH {
        return None;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if !["node_modules", ".git", "obj", "bin"].contains(&name) {
                subdirs.push(path);
            }
            continue;
        }
        if name.ends_with(".csproj") && csproj_has_ef(&path) {
            let rel = path
                .strip_prefix(cwd)
                .unwrap_or(&path)
                .display()
                .to_string();
            return Some(rel);
        }
    }
    for sub in subdirs {
        if let Some(rel) = find_csproj_with_ef_in(cwd, &sub, depth + 1) {
            return Some(rel);
        }
    }
    None
}

fn csproj_has_ef(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|content| {
        content.contains("Microsoft.EntityFrameworkCore")
            || content.contains("Microsoft.EntityFrameworkCore.Design")
    })
}

fn check_dotnet(_ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if which("dotnet") {
        emit("dotnet found on PATH");
        Ok(RunOutcome::Ran("dotnet installed".into()))
    } else {
        Err(RunError::Failed(
            "dotnet not found on PATH. Install the .NET SDK via https://dotnet.microsoft.com/download".into(),
        ))
    }
}

fn dotnet_restore(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    // Install the local `dotnet-ef` tool manifest if present; then restore
    // packages. Both are idempotent. The tool manifest lives next to the
    // detected project, so resolve it from there when the project is nested.
    if let Some((proj_rel, proj_dir)) = located_project(&ctx.cwd) {
        let manifest = proj_dir.join(".config").join("dotnet-tools.json");
        if manifest.is_file() {
            spawn_cmd("dotnet", &["tool", "restore"], &proj_dir, emit)?;
        }
        return spawn_cmd("dotnet", &["restore", &proj_rel], &ctx.cwd, emit);
    }
    if ctx.cwd.join(".config").join("dotnet-tools.json").is_file() {
        spawn_cmd("dotnet", &["tool", "restore"], &ctx.cwd, emit)?;
    }
    spawn_cmd("dotnet", &["restore"], &ctx.cwd, emit)
}

fn ef_database_update(ctx: &Context, emit: &mut dyn FnMut(&str)) -> Result<RunOutcome, RunError> {
    if !which("dotnet") {
        return Err(RunError::Failed(
            "dotnet not found on PATH, cannot run 'dotnet ef database update'".into(),
        ));
    }
    match located_project(&ctx.cwd) {
        Some((proj_rel, _)) => spawn_cmd(
            "dotnet",
            &[
                "ef",
                "database",
                "update",
                "--project",
                &proj_rel,
                "--startup-project",
                &proj_rel,
            ],
            &ctx.cwd,
            emit,
        ),
        None => spawn_cmd("dotnet", &["ef", "database", "update"], &ctx.cwd, emit),
    }
}

/// Returns the detected project's path relative to `cwd` and its absolute
/// directory (the `.csproj` parent), so tasks target the real application
/// instead of running in the workspace root.
fn located_project(cwd: &Path) -> Option<(String, std::path::PathBuf)> {
    let rel = find_csproj_with_ef(cwd)?;
    let dir = cwd.join(&rel).parent().map(Path::to_path_buf)?;
    Some((rel, dir))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("upone-ef-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_csprojo_with_ef_reference() {
        let dir = temp_dir("ok");
        fs::write(
            dir.join("App.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk.Web">
  <ItemGroup>
    <PackageReference Include="Microsoft.EntityFrameworkCore.Sqlite" Version="9.0.0" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();
        let rel = find_csproj_with_ef(&dir).unwrap();
        assert_eq!(rel, "App.csproj");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_nested_project() {
        let dir = temp_dir("nested");
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src").join("App.csproj"),
            "<Project><ItemGroup><PackageReference Include=\"Microsoft.EntityFrameworkCore\" /></ItemGroup></Project>",
        )
        .unwrap();
        let rel = find_csproj_with_ef(&dir).unwrap();
        assert_eq!(rel, "src/App.csproj");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_projects_without_ef() {
        let dir = temp_dir("plain");
        fs::write(
            dir.join("App.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk.Web"></Project>"#,
        )
        .unwrap();
        assert!(find_csproj_with_ef(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
