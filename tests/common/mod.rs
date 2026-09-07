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

/// A `comply` command whose `$HOME` is the target dir.
/// Its telemetry lands there, not in `~/.comply`.
/// E2E fixtures must never pollute real stats.
pub fn comply() -> assert_cmd::Command {
    let mut command = assert_cmd::Command::cargo_bin("comply").expect("comply binary");
    command.env("HOME", env!("CARGO_TARGET_TMPDIR"));
    command
}
