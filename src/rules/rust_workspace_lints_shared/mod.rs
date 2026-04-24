//! rust-workspace-lints-shared — workspaces must share a single lint
//! policy via `[workspace.lints]`, and member crates must opt in with
//! `[lints] workspace = true`.
//!
//! Without this discipline, each crate silently diverges on clippy
//! categories, `deny(warnings)`, MSRV lints, etc. The workspace lints
//! feature (stable in Cargo 1.74) exists precisely to centralise that
//! policy.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-workspace-lints-shared",
    description: "Workspace lacks `[workspace.lints]` or member crate doesn't inherit it.",
    remediation: "In the workspace root `Cargo.toml`, declare a `[workspace.lints.*]` \
                  section (clippy, rust, rustdoc). In every member crate, add \
                  `[lints]` followed by `workspace = true` to inherit the policy. \
                  Prevents per-crate drift of clippy categories and `deny(warnings)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Toml, Backend::Text(Box::new(text::Check)))],
    }
}
