//! Reading and writing CHANGELOG.md files.

use std::path::Path;

use anyhow::{Context, Result};

const HEADER: &str = "# Changelog";

/// Ensures a changelog exists with a top-level header.
pub fn ensure(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{HEADER}\n\n"))
        .with_context(|| format!("create {}", path.display()))?;
    Ok(())
}

/// Prepend a `## <version>` section (with bullet lines) right after the header.
pub fn prepend_section(path: &Path, version: &str, bullets: &[String]) -> Result<()> {
    let mut block = format!("## {version}\n\n");
    for b in bullets {
        block.push_str(&format!("- {b}\n"));
    }
    prepend_raw(path, version, &block)
}

/// Prepend an arbitrary block whose first heading is `## <version>`.
pub fn prepend_raw(path: &Path, _version: &str, block: &str) -> Result<()> {
    ensure(path)?;
    let existing = std::fs::read_to_string(path)?;
    let (preamble, rest) = match existing.split_once("\n## ") {
        Some((pre, rem)) => (pre.to_string(), format!("\n## {rem}")),
        None => (existing, String::new()),
    };
    let mut out = preamble.trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(block.trim_end());
    out.push('\n');
    out.push_str(rest.trim_start_matches('\n'));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// Extracts the bullet lines of a `## <version>` section.
pub fn read_section(path: &Path, version: &str) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let header = format!("## {version}");
    let mut bullets = Vec::new();
    let mut in_section = false;
    for line in content.split('\n') {
        let t = line.trim();
        if t == header || t == format!("#{version}") {
            in_section = true;
            continue;
        }
        if in_section && t.starts_with('#') {
            break;
        }
        if in_section {
            let t = t.strip_prefix("- ").unwrap_or(t);
            if !t.is_empty() {
                bullets.push(t.to_string());
            }
        }
    }
    Ok(bullets)
}
