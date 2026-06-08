//! rust-asref-path-for-fs-fns — functions that touch the filesystem
//! should accept `impl AsRef<Path>` rather than a concrete `&Path`,
//! `&str` or `PathBuf`.
//!
//! `AsRef<Path>` is the standard library's pattern for "anything
//! path-shaped" — callers can pass `&str`, `String`, `&Path`,
//! `PathBuf`, `&OsStr`, `Cow<Path>` without ceremony. Pinning the
//! parameter to one concrete type forces every caller to convert,
//! producing noise at the boundary and sometimes unnecessary
//! allocations.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-asref-path-for-fs-fns",
    description: "Filesystem fn takes a concrete path type instead of `impl AsRef<Path>`.",
    remediation: "Change the parameter to `impl AsRef<Path>` (or \
                  `P: AsRef<Path>` via a generic) so callers can pass \
                  `&str`, `String`, `&Path` or `PathBuf` without \
                  converting. Matches `std::fs` conventions and \
                  avoids needless allocations at the call site.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
