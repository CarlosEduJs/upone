//! Reading and writing `.changes/` notes.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};

/// Semantic bump severity. Order matters: Patch < Minor < Major.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl FromStr for Bump {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "patch" => Ok(Bump::Patch),
            "minor" => Ok(Bump::Minor),
            "major" => Ok(Bump::Major),
            other => Err(format!(
                "invalid bump type: {other} (expected patch, minor or major)"
            )),
        }
    }
}

impl fmt::Display for Bump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bump::Patch => f.write_str("patch"),
            Bump::Minor => f.write_str("minor"),
            Bump::Major => f.write_str("major"),
        }
    }
}

/// A single parsed changeset note.
#[derive(Debug)]
pub struct Note {
    pub path: PathBuf,
    /// Canonical package name (upone, upone-core, upone-providers).
    pub package: String,
    pub bump: Bump,
    pub summary: String,
}

const KNOWN: &[(&str, &str)] = &[
    ("upone", "upone"),
    ("cli", "upone"),
    ("upone-core", "upone-core"),
    ("core", "upone-core"),
    ("upone-providers", "upone-providers"),
    ("providers", "upone-providers"),
];

/// Normalizes a crate alias (e.g. "cli") to its package name (e.g. "upone").
pub fn resolve_package(alias: &str) -> Result<String> {
    let key = alias.trim().to_lowercase();
    KNOWN
        .iter()
        .find(|(a, _)| *a == key)
        .map(|(_, p)| p.to_string())
        .ok_or_else(|| {
            anyhow!("unknown crate: {alias} (expected one of upone, upone-core, upone-providers)")
        })
}

/// Reads all active notes directly under `.changes/` (non-recursive).
///
/// Files named `README.md` and the `archive/` subdirectory are ignored.
pub fn read_notes(root: &Path) -> Result<Vec<Note>> {
    let dir = root.join(".changes");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut notes = Vec::new();
    for entry in std::fs::read_dir(&dir).context("reading .changes/")? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("README.md") {
            continue;
        }
        if let Some(note) = parse_note(&path)? {
            notes.push(note);
        }
    }
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(notes)
}

fn parse_note(path: &Path) -> Result<Option<Note>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let lines: Vec<&str> = content.split('\n').map(str::trim_end).collect();

    if lines.first().map(|l| l.trim()) != Some("---") {
        bail!(
            "note {} must start with `---` frontmatter (crate + bump)",
            path.display()
        );
    }

    let mut end = 1;
    while end < lines.len() && lines[end].trim() != "---" {
        end += 1;
    }
    if end >= lines.len() {
        bail!(
            "note {} has an unclosed `---` frontmatter block",
            path.display()
        );
    }

    let mut crate_raw: Option<String> = None;
    let mut bump_raw: Option<String> = None;
    for line in &lines[1..end] {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once(':').unwrap_or((line, ""));
        let v = v.trim().trim_matches('"').trim_matches('\'');
        match k.trim().to_lowercase().as_str() {
            "crate" => crate_raw = Some(v.to_string()),
            "bump" => bump_raw = Some(v.to_string()),
            _ => {}
        }
    }

    let raw_crate = crate_raw
        .ok_or_else(|| anyhow!("note {} is missing the `crate:` field", path.display()))?;
    let package = resolve_package(&raw_crate)?;
    let raw_bump =
        bump_raw.ok_or_else(|| anyhow!("note {} is missing the `bump:` field", path.display()))?;
    let bump: Bump = raw_bump.parse().map_err(|e| anyhow!("{e}"))?;

    let summary = lines[end + 1..]
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if summary.is_empty() {
        bail!("note {} has an empty summary", path.display());
    }

    Ok(Some(Note {
        path: path.to_path_buf(),
        package,
        bump,
        summary,
    }))
}

/// Creates a new note file under `.changes/`.
pub fn new_note(root: &Path, alias: &str, bump: Bump, summary: &str) -> Result<()> {
    let package = resolve_package(alias)?;
    let dir = root.join(".changes");
    std::fs::create_dir_all(&dir)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let slug = slugify(summary);
    let path = dir.join(format!("{package}-{ts}-{slug}.md"));
    let content = format!("---\ncrate: {package}\nbump: {bump}\n---\n\n{summary}\n");
    std::fs::write(&path, content)?;
    println!("created note: {}", path.display());
    Ok(())
}

fn slugify(s: &str) -> String {
    let words: Vec<String> = s
        .split_whitespace()
        .take(6)
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        "note".to_string()
    } else {
        words.join("-")
    }
}

/// Moves consumed notes into `.changes/archive/<version>/`.
pub fn archive_notes(root: &Path, version: &str, notes: &[Note]) -> Result<()> {
    if notes.is_empty() {
        return Ok(());
    }
    let dir = root.join(".changes").join("archive").join(version);
    std::fs::create_dir_all(&dir)?;
    for note in notes {
        let name = note
            .path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("note path has no filename"))?;
        let dest = dir.join(name);
        std::fs::rename(&note.path, &dest)?;
    }
    Ok(())
}
