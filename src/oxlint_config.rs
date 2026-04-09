//! Embedded oxlint config + secure temp file writer.
//!
//! Why this module exists: comply ships its own oxlint config baked into the
//! binary so users don't need a separate `.oxlintrc.json`. We write it to a
//! per-invocation temp file at runtime because oxlint needs a real path.
//!
//! Security notes:
//! - We use `tempfile::NamedTempFile` rather than a shared `/tmp/comply/`
//!   path. The unpredictable filename + `O_EXCL` mode prevents the classic
//!   `/tmp` symlink attack where a malicious user pre-creates the path as a
//!   symlink to a victim-writable file.
//! - Concurrent comply invocations can't clobber each other's config —
//!   each gets its own temp file that's deleted on drop.

use anyhow::{Context, Result};
use std::io::Write;

/// Embedded oxlint config — built into the binary at compile time.
const OXLINTRC: &str = include_str!("oxlintrc.json");

/// Write the embedded oxlintrc to a fresh secure temp file and return it.
/// The returned `NamedTempFile` deletes itself on drop, so the caller just
/// keeps it alive for as long as the oxlint subprocess needs to read it.
pub fn write() -> Result<tempfile::NamedTempFile> {
    let mut tmp = tempfile::Builder::new()
        .prefix("comply-")
        .suffix(".json")
        .tempfile()
        .context("failed to create temp oxlint config")?;
    tmp.write_all(OXLINTRC.as_bytes())
        .context("failed to write oxlint config to temp file")?;
    tmp.flush()
        .context("failed to flush temp oxlint config to disk")?;
    Ok(tmp)
}
