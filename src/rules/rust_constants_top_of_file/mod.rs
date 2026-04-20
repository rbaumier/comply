//! rust-constants-top-of-file — module-level `const` / `static`
//! declarations must appear before any `fn` / `struct` / `enum` /
//! `impl` / `trait` / `mod` / `type` / `union` at the module level.
//!
//! Rationale: a reader opening a file expects the configuration
//! knobs (thresholds, magic numbers, IDs, well-known strings) to
//! live in a clearly identifiable header, not buried between
//! function bodies. `use` and `extern crate` items are transparent
//! — they may appear before or after the constants.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-constants-top-of-file",
    description: "Module-level `const` / `static` must appear before any `fn` / `struct` / `impl`.",
    remediation: "Move this `const` / `static` to the top of the file, after the `use` \
                  declarations and before the first `fn` / `struct` / `impl`. Top-of-file \
                  constants answer \"what knobs does this module have?\" without having \
                  to grep.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
