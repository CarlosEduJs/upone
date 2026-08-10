#![allow(clippy::unwrap_used)]

//! Shared helpers for provider tests: unique scratch directories.

use std::path::PathBuf;

/// Creates a fresh, empty temp directory unique to this test run.
///
/// The directory name embeds the caller's `prefix` and `name` plus the process
/// id, so parallel tests never collide. Any previous directory with the same
/// name is removed first.
pub fn temp_dir(prefix: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("upone-{prefix}-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
