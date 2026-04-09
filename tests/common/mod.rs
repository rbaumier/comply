//! Shared helpers for E2E tests.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Write a TS file in a temp dir and return the dir + path.
/// The TempDir must be held by the caller — its Drop deletes the directory.
pub fn write_ts_file(name: &str, content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join(name);
    fs::write(&path, content).expect("failed to write fixture");
    (dir, path)
}
