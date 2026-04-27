//! rust-eprintln-in-library — `eprintln!` in library code.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-eprintln-in-library",
    description: "`eprintln!` / `eprint!` invoked from library code.",
    remediation: "Library code shouldn't write to stderr directly — \
                  consumers can't redirect, configure, or capture it. \
                  Use `tracing::warn!` / `tracing::error!` so the \
                  consumer's subscriber controls the output.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
