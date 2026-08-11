//! Shared helpers to run commands safely and explainably.

use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, SystemTime};

use upone_core::plan::{Planner, RunOutcome, Task};
use upone_core::readiness::{resolve_env_key, Importance, ReadinessCheck, ReadinessStatus};
use upone_core::run::RunError;
use upone_core::Context;

/// Runs a command with the given args in `cwd`, streaming output to `emit`.
pub fn spawn_cmd(
    program: &str,
    args: &[&str],
    cwd: &Path,
    emit: &mut dyn FnMut(&str),
) -> Result<RunOutcome, RunError> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;

    if let Ok(stderr) = String::from_utf8(output.stderr.clone()) {
        if !stderr.trim().is_empty() {
            emit(&stderr);
        }
    }
    let stdout = String::from_utf8(output.stdout).unwrap_or_default();
    if !stdout.trim().is_empty() {
        emit(&stdout);
    }

    if output.status.success() {
        Ok(RunOutcome::Ran)
    } else {
        // Some tools (e.g. pnpm) report errors on stdout, so fall back to
        // it when stderr is empty. Use the tail of the output: the real
        // failure message usually shows up last (compose pull/start lines
        // precede the actual error).
        let stderr = String::from_utf8(output.stderr).unwrap_or_default();
        let lines: Vec<String> = stderr
            .lines()
            .chain(stdout.lines())
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        let start = lines.len().saturating_sub(3);
        let detail = lines[start..].join(" | ");
        Err(RunError::Command {
            program: program.to_string(),
            args: args.join(" "),
            exit: output.status.code(),
            detail: if detail.is_empty() {
                "no output".to_string()
            } else {
                detail
            },
        })
    }
}

/// Checks if a program exists on PATH.
#[must_use]
pub fn which(program: &str) -> bool {
    which_probe(program, "--version")
}

/// Like [`which`] but probes with an explicit argument, for tools that reject
/// `--version` (e.g. Go: the valid probe is `go version`).
#[must_use]
pub fn which_probe(program: &str, probe: &str) -> bool {
    probe_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((program.to_string(), probe.to_string()))
        .or_insert_with(|| {
            Command::new(program)
                .arg(probe)
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .to_owned()
}

/// Result of a binary probe, cached so PATH checks spawn a process only once
/// per (program, probe) pair per run.
type ProbeKey = (String, String);

fn probe_cache() -> &'static Mutex<HashMap<ProbeKey, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<ProbeKey, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// ── Cached file reads ───────────────────────────────────────────────────────

/// A file's contents as last read, keyed by the modification time (and size)
/// observed at that moment. A change on disk forces a fresh read, so repeated
/// detection across providers and packages stops re-reading the same file but
/// never serves stale content.
struct CachedFile {
    mtime: Option<SystemTime>,
    len: u64,
    raw: Arc<str>,
    lower: Arc<str>,
    #[cfg(test)]
    reads: usize,
}

fn file_cache() -> &'static Mutex<HashMap<PathBuf, CachedFile>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedFile>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Reads `path` once per (path, mtime, size) identity, returning the raw
/// contents and a lowercased copy. Missing files stay uncached (each call
/// stats the path), so a file created later is picked up immediately.
fn cached_file(path: &Path) -> Option<(Arc<str>, Arc<str>)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok();
    let len = meta.len();

    {
        let cache = file_cache().lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = cache.get(path) {
            if entry.mtime == mtime && entry.len == len {
                return Some((entry.raw.clone(), entry.lower.clone()));
            }
        }
    }

    let raw = Arc::<str>::from(std::fs::read_to_string(path).ok()?);
    let lower = Arc::<str>::from(raw.to_lowercase());
    #[cfg(test)]
    let prev_reads = cache_reads(path) + 1;

    let mut cache = file_cache().lock().unwrap_or_else(PoisonError::into_inner);
    cache.insert(
        path.to_path_buf(),
        CachedFile {
            mtime,
            len,
            raw: raw.clone(),
            lower: lower.clone(),
            #[cfg(test)]
            reads: prev_reads,
        },
    );
    drop(cache);
    Some((raw, lower))
}

#[cfg(test)]
fn cache_reads(path: &Path) -> usize {
    file_cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(path)
        .map_or(0, |e| e.reads)
}

/// Returns true if `needle` appears (case-insensitive) anywhere in the given files.
pub fn files_contain(cwd: &Path, files: &[&str], needles: &[&str]) -> bool {
    let needles: Vec<String> = needles.iter().map(|n| n.to_lowercase()).collect();
    files.iter().any(|file| {
        cached_file(&cwd.join(file))
            .is_some_and(|(_, lower)| needles.iter().any(|n| lower.contains(n)))
    })
}

/// Returns true if any of the files exists in `cwd`.
pub fn any_exists(cwd: &Path, files: &[&str]) -> bool {
    files.iter().any(|f| cwd.join(f).exists())
}

/// Returns the Python install task id detected in the project (if any).
/// Used by providers that depend on the project's deps being installed
/// (alembic, sqlalchemy).
///
/// Mirrors [`js_install_task`]: walks up from `cwd` to the filesystem root so a
/// monorepo package can depend on the root's install task.
pub fn python_install_task(cwd: &Path) -> Option<&'static str> {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let id = if d.join("uv.lock").is_file() {
            Some("uv-sync")
        } else if d.join("poetry.lock").is_file() {
            Some("poetry-install")
        } else if super::python::has_requirements(d) {
            Some("pip-install")
        } else {
            None
        };
        if let Some(id) = id {
            return Some(id);
        }
        dir = d.parent();
    }
    None
}

/// Resolves the id of the database task that starts/verifies the DB an ORM
/// migration targets, so a migration runs only after its DB is up.
///
/// Matches the same signals the DB providers use: a `postgres`/`mysql`/
/// `mariadb`/`mongo` service in docker-compose, or a sqlite `DATABASE_URL`.
/// Returns `None` when the DB is external (no provider task exists — the
/// migration will simply try against it).
///
/// Mirrors [`python_install_task`] / [`js_install_task`]: walks up from `cwd`
/// to the filesystem root, so a nested package picks up the DB defined by a
/// compose file or `.env` at the workspace root.
pub fn migration_db_dep(cwd: &Path) -> Option<&'static str> {
    let compose = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ];
    let envs = [".env", ".env.local"];
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if files_contain(d, &compose, &["postgres", "postgresql"]) {
            return Some("postgres-up");
        }
        if files_contain(d, &compose, &["mysql", "mariadb"]) {
            return Some("mysql-up");
        }
        if files_contain(d, &compose, &["mongo", "mongodb"]) {
            return Some("mongo-up");
        }
        if files_contain(d, &envs, &["sqlite://", "DATABASE_URL=sqlite"]) {
            return Some("sqlite-ensure");
        }
        dir = d.parent();
    }
    None
}

/// Resolves the host port a service is published on in the given compose files.
///
/// Matches port mappings like `- "15432:5432"` / `- 15432:5432` where the
/// *container* side equals `container_port` and returns the *host* side, so
/// providers can verify the real published port instead of assuming 5432/6379.
/// Handles IP-bound mappings (`- "127.0.0.1:15432:5432"`) by taking the
/// host-port segment. Falls back to `container_port` when nothing is
/// configured or parseable.
pub fn compose_host_port(cwd: &Path, files: &[&str], container_port: u16) -> u16 {
    let needle = format!(":{container_port}");
    for file in files {
        let Some((content, _)) = cached_file(&cwd.join(file)) else {
            continue;
        };
        for line in content.lines() {
            let Some(pos) = line.find(&needle) else {
                continue;
            };
            // Everything before the colon is either just the host port
            // (`15432`) or an IP-bound form (`127.0.0.1:15432`); the segment
            // right before the container port is the host port in both.
            let mut host = line[..pos].trim();
            host = host.trim_matches(['"', '\'', '-', ' ', ':', '[', ']', '+']);
            let host_port = host.rsplit(':').next().unwrap_or(host);
            if let Ok(port) = host_port.trim().parse::<u16>() {
                return port;
            }
        }
    }
    container_port
}

/// Returns true if `dependency` is a key in the package.json dependencies,
/// devDependencies, peerDependencies or optionalDependencies maps of `cwd`.
/// Used for dependency-based detections (trpc, better-auth, next) that have no
/// dedicated config file. Parses the manifest as JSON so unrelated keys (e.g.
/// `scripts.next` or `overrides.next`) can never trigger a match.
pub fn package_has_dependency(cwd: &Path, dependency: &str) -> bool {
    let Some((text, _)) = cached_file(&cwd.join("package.json")) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .any(|section| {
        value
            .get(*section)
            .and_then(|map| map.as_object())
            .is_some_and(|map| map.contains_key(dependency))
    })
}

/// Returns true when a `node_modules` directory exists at or above `cwd`.
///
/// Workspace roots hoist dependencies, so a package may not hold its own
/// `node_modules` (npm/pnpm/yarn hoist to the root, bun links them there too).
/// Mirrors the upward-resolution behavior of [`js_install_task`].
pub fn node_modules_present(cwd: &Path) -> bool {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if d.join("node_modules").is_dir() {
            return true;
        }
        dir = d.parent();
    }
    false
}

/// Returns true when a bare install task id exists for the project, i.e. the
/// project is managed by a detected JS package manager (a lockfile exists at or
/// above `cwd`). Providers use this to guarantee their tasks can depend on the
/// install task instead of running against a project with no recorded deps.
pub fn js_managed(cwd: &Path) -> bool {
    js_install_task(cwd).is_some()
}

/// Returns true when a CLI that ships on `node_modules/.bin/<bin>` is present at
/// or above `cwd` (mirroring hoisted workspaces). Lets migration providers
/// require a *local* CLI rather than printing an npx registry prompt.
pub fn local_cli(cwd: &Path, bin: &str) -> bool {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let candidate = d.join("node_modules").join(".bin").join(bin);
        if candidate.is_file() || candidate.is_symlink() {
            return true;
        }
        dir = d.parent();
    }
    false
}

/// Reports whether `host:port` accepts a TCP connection.
///
/// Resolves the host through the system resolver (so it handles `localhost`,
/// IP literals and hostnames alike) and tries each address with a 300ms
/// timeout. Shared by the postgres/redis/mysql/mongo providers.
#[must_use]
pub fn tcp_reachable(host: &str, port: u16) -> bool {
    format!("{host}:{port}")
        .to_socket_addrs()
        .is_ok_and(|mut addrs| {
            addrs
                .find_map(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok())
                .is_some()
        })
}

/// Parses the `host[:port]` authority of a `<scheme>://` connection URL.
///
/// Accepts any of `schemes`, drops credentials (`user@`), a path and a query
/// string, and defaults the port when absent. Returns `None` when the URL uses
/// none of the schemes or has no usable authority (e.g. a unix socket).
/// Shared by the mysql and mongo providers.
#[must_use]
pub fn parse_uri_authority(
    url: &str,
    schemes: &[&str],
    default_port: u16,
) -> Option<(String, u16)> {
    let url = url.split_once('?').map_or(url, |(u, _)| u);
    let authority = schemes
        .iter()
        .find_map(|s| url.strip_prefix(&format!("{s}://")))?
        .split('/')
        .next()?
        .rsplit('@')
        .next()?;
    if authority.is_empty() || authority.starts_with('/') {
        return None;
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok() => {
            Some((host.to_string(), port.parse::<u16>().ok()?))
        }
        _ => Some((authority.to_string(), default_port)),
    }
}

/// Shared "binary on PATH" check with an install hint on failure.
///
/// # Errors
///
/// Fails with a `RunError::Failed` when `bin` is not on PATH.
pub fn check_binary(
    bin: &str,
    hint: &str,
    emit: &mut dyn FnMut(&str),
) -> Result<RunOutcome, RunError> {
    if which(bin) {
        emit(&format!("{bin} found on PATH"));
        Ok(RunOutcome::Ran)
    } else {
        Err(RunError::Failed(format!("{bin} not found on PATH. {hint}")))
    }
}

/// Like [`check_binary`] but probes with an explicit argument, for tools that
/// reject `--version` (e.g. Go: the valid probe is `go version`).
///
/// # Errors
///
/// Fails with a `RunError::Failed` when `bin` is not on PATH.
pub fn check_binary_probe(
    bin: &str,
    probe: &str,
    hint: &str,
    emit: &mut dyn FnMut(&str),
) -> Result<RunOutcome, RunError> {
    if which_probe(bin, probe) {
        emit(&format!("{bin} found on PATH"));
        Ok(RunOutcome::Ran)
    } else {
        Err(RunError::Failed(format!("{bin} not found on PATH. {hint}")))
    }
}

/// Builds a readiness check asserting that a `node_modules` directory exists at
/// or above `cwd`. Shared by the yarn/drizzle/knex/sequelize providers, whose
/// only differences are the task id, display name and install remedy.
#[must_use]
pub fn node_modules_check(id: &str, tool: &str, remedy: &str, cwd: &Path) -> ReadinessCheck {
    let cwd = cwd.to_path_buf();
    let id = id.to_string();
    let tool = tool.to_string();
    let remedy = remedy.to_string();
    ReadinessCheck::new(
        id,
        format!("{tool} dependencies installed"),
        format!("node_modules present for {tool}"),
        Importance::Required,
        move |_ctx| {
            if node_modules_present(&cwd) {
                ReadinessStatus::Ready("node_modules present".into())
            } else {
                ReadinessStatus::NotReady {
                    reason: format!("node_modules missing for {tool}"),
                    remedy: remedy.clone(),
                }
            }
        },
    )
}

/// Builds a readiness check asserting that an environment key is set.
/// Shared by the postgres/mysql/better-auth providers, whose only differences
/// are the task id and the key name.
#[must_use]
pub fn env_key_check(id: &str, key: &str, cwd: &Path) -> ReadinessCheck {
    let cwd = cwd.to_path_buf();
    let id = id.to_string();
    let key = key.to_string();
    ReadinessCheck::new(
        id,
        key.clone(),
        format!("{key} environment variable is set"),
        Importance::Required,
        move |_ctx| {
            if resolve_env_key(&cwd, &key).is_some() {
                ReadinessStatus::Ready("found".into())
            } else {
                ReadinessStatus::NotReady {
                    reason: format!("{key} not found in process env or .env* files"),
                    remedy: format!("Add {key} to your .env.local or shell environment"),
                }
            }
        },
    )
}

/// Which install task a provider's plan should depend on.
#[derive(Clone, Copy)]
pub enum InstallKind {
    Js,
    Python,
}

/// Whether an ORM/db provider's action task should be wired to depend on the
/// database task detected in the project (e.g. `postgres-up`), resolved by
/// [`add_migration_plan`] from the project files.
#[derive(Clone, Copy)]
pub enum DbWiring {
    /// The action only needs the install task (client generation, etc.).
    None,
    /// Also wire the detected database task (if any) into the action's deps.
    Database,
}

/// Registers a `check` + `action` (migrate/generate) pair for the ORM/db
/// providers, wiring the install and database dependencies that are only known
/// after detection. Providers pass fully-built tasks and the helper wires
/// them in; `install` resolves the install task id for the project, and
/// `db_wiring` (when [`DbWiring::Database`]) adds the detected DB task as an
/// action dependency.
pub fn add_migration_plan(
    planner: &mut Planner<'_>,
    ctx: &Context,
    check: Task,
    action: Task,
    install: InstallKind,
    db_wiring: DbWiring,
) {
    let check_id = check.id.clone();
    let mut action_deps = vec![check_id];
    let mut check = check;
    let install = match install {
        InstallKind::Js => js_install_task(&ctx.cwd),
        InstallKind::Python => python_install_task(&ctx.cwd),
    };
    if let Some(install) = install {
        action_deps.push(install.to_string());
        check = check.depends_on([install]);
    }
    if matches!(db_wiring, DbWiring::Database) {
        if let Some(db) = migration_db_dep(&ctx.cwd) {
            action_deps.push(db.to_string());
        }
    }
    planner.add(check);
    planner.add(action.depends_on(action_deps));
}

/// Resolves the JS install task id detected in the project (if any).
/// Used by providers that depend on `node_modules` (prisma, drizzle).
///
/// In a monorepo the lockfile usually lives at the workspace root while the
/// package sits deeper, so this walks up from `cwd` to the filesystem root.
pub fn js_install_task(cwd: &Path) -> Option<&'static str> {
    let markers: [(&str, &str); 5] = [
        ("bun.lock", "bun-install"),
        ("bun.lockb", "bun-install"),
        ("yarn.lock", "yarn-install"),
        ("pnpm-lock.yaml", "pnpm-install"),
        ("package-lock.json", "npm-install"),
    ];
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if let Some((_, id)) = markers.iter().find(|(f, _)| d.join(f).is_file()) {
            return Some(*id);
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        crate::testkit::temp_dir("cmd", name)
    }

    #[test]
    fn package_has_dependency_checks_dep_maps_only() {
        let dir = temp_dir("dep");
        fs::write(
            dir.join("package.json"),
            r#"{
  "name": "pkg",
  "scripts": { "next": "echo nope" },
  "overrides": { "better-auth": "1.0.0" },
  "dependencies": { "next": "15.0.0" },
  "devDependencies": { "@trpc/server": "^11.0.0" },
  "peerDependencies": { "zod": "4.0.0" },
  "optionalDependencies": { "esbuild": "0.25.0" }
}"#,
        )
        .unwrap();
        assert!(package_has_dependency(&dir, "next"));
        assert!(package_has_dependency(&dir, "@trpc/server"));
        assert!(package_has_dependency(&dir, "zod"));
        assert!(package_has_dependency(&dir, "esbuild"));
        // Present only in scripts/overrides must not match.
        assert!(!package_has_dependency(&dir, "better-auth"));
        assert!(!package_has_dependency(&dir, "echo"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_has_dependency_missing_or_broken_manifest() {
        let dir = temp_dir("bad");
        assert!(!package_has_dependency(&dir, "next"));
        fs::write(dir.join("package.json"), "{not json").unwrap();
        assert!(!package_has_dependency(&dir, "next"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_modules_present_walks_up() {
        let dir = temp_dir("nm").join("a").join("b");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(temp_dir("nm").join("node_modules")).unwrap();
        assert!(node_modules_present(&dir));
        let _ = fs::remove_dir_all(temp_dir("nm"));
    }

    #[test]
    fn js_install_task_resolves_yarn_last_resort() {
        let dir = temp_dir("yarn");
        fs::write(dir.join("yarn.lock"), "# autogenerated").unwrap();
        assert_eq!(js_install_task(&dir), Some("yarn-install"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compose_host_port_handles_plain_and_ip_bound() {
        let dir = temp_dir("ports");
        fs::write(
            dir.join("compose.yml"),
            "services:\n  db:\n    ports:\n      - \"127.0.0.1:15432:5432\"\n",
        )
        .unwrap();
        let files = ["compose.yml", "docker-compose.yml"];
        // IP-bound: the host port published on the interface wins over 5432.
        assert_eq!(compose_host_port(&dir, &files, 5432), 15432);
        // Nothing matching that container port -> fallback unchanged.
        assert_eq!(compose_host_port(&dir, &files, 6379), 6379);
        let _ = fs::remove_dir_all(&dir);
    }

    fn cache_path(name: &str) -> PathBuf {
        let dir = temp_dir(name);
        fs::create_dir_all(&dir).unwrap();
        dir.join("compose.yml")
    }

    #[test]
    fn cached_read_deduplicates_and_refreshes_on_change() {
        let file = cache_path("cache");
        fs::write(&file, "services:\n  db:\n    image: postgres\n").unwrap();

        let dir = file.parent().unwrap();
        let before = cache_reads(&file);
        assert!(files_contain(dir, &["compose.yml"], &["postgres"]));
        assert_eq!(cache_reads(&file), before + 1, "first read hits the disk");

        assert!(files_contain(dir, &["compose.yml"], &["POSTGRES"]));
        assert_eq!(
            cache_reads(&file),
            before + 1,
            "subsequent reads must not re-open the file"
        );

        // A change on disk (mtime and/or size) forces a fresh read, so stale
        // content is never served after a setup step rewrites the file.
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&file, "services:\n  cache:\n    image: redis\n").unwrap();
        assert!(files_contain(dir, &["compose.yml"], &["redis"]));
        assert!(!files_contain(dir, &["compose.yml"], &["postgres"]));
        assert!(
            cache_reads(&file) > before + 1,
            "an mtime change must re-read the file"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cached_read_serves_raw_parseable_content() {
        let dir = temp_dir("cache-dep");
        fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"next":"15.0.0"}}"#,
        )
        .unwrap();
        let before = cache_reads(&dir.join("package.json"));
        assert!(package_has_dependency(&dir, "next"));
        assert!(package_has_dependency(&dir, "next"));
        assert_eq!(cache_reads(&dir.join("package.json")), before + 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cargo_commands_report_exit_diagnostic() {
        let dir = temp_dir("runerr");
        let mut emit = |_: &str| {};
        let err = spawn_cmd("false", &[], &dir, &mut emit).unwrap_err();
        assert!(matches!(err, RunError::Command { .. }));
        let _ = fs::remove_dir_all(&dir);
    }
}
