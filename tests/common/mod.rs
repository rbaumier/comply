//! Shared helpers for E2E tests.

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The comply.toml a fixture writes to opt out of the type-aware phase.
///
/// Those rules need a resolved TypeScript program, which needs a tsconfig.json
/// and `@typescript/native-preview` resolvable from the project. A temp fixture
/// has neither, and comply refuses a phase it cannot run instead of reporting a
/// file it never type-checked. A fixture testing text and AST rules declares
/// that, rather than leaning on a silent skip.
const WITHOUT_TYPE_AWARE: &str = "\
[rules.no-duplicate-type-definition]
disabled = true

[rules.no-redundant-nullish-coalescing-null]
disabled = true

[rules.ts-no-in-operator]
disabled = true

[rules.ts-no-typeof-operator]
disabled = true
";

/// Declare in `dir` that this fixture does not want the type-aware rules.
pub fn without_type_aware_rules(dir: &Path) {
    fs::write(dir.join("comply.toml"), WITHOUT_TYPE_AWARE).expect("failed to write comply.toml");
}

/// Write a TS file in a temp dir and return the dir + path.
/// The TempDir must be held by the caller — its Drop deletes the directory.
pub fn write_ts_file(name: &str, content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("failed to create temp dir");
    without_type_aware_rules(dir.path());
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
