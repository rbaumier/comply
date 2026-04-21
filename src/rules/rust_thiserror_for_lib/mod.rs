//! rust-thiserror-for-lib — library error enums should derive `thiserror::Error`.
//!
//! Library crates that hand-roll `impl Display` + `impl Error` for
//! every variant drown in boilerplate and tend to miss the
//! `#[source]` chain. `#[derive(thiserror::Error)]` + per-variant
//! `#[error("…")]` attributes cover both, preserve source chaining,
//! and keep the enum the single source of truth.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-thiserror-for-lib",
    description: "Library error types should derive `thiserror::Error` instead of manually implementing `Display`.",
    remediation: "Add `#[derive(thiserror::Error)]` and use `#[error(\"...\")]` attributes on enum variants.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
