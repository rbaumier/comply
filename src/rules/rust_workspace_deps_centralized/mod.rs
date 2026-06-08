//! rust-workspace-deps-centralized — member crates must inherit
//! dependencies from the workspace rather than pinning them individually.
//!
//! In a Cargo workspace, the canonical pattern is to declare each
//! third-party crate version once under `[workspace.dependencies]` and
//! let every member crate opt in with `foo = { workspace = true }`.
//! Scattering `foo = "1.2.3"` across member `Cargo.toml` files lets
//! versions drift, breaks feature unification and makes bumping a
//! shared dep a multi-file hunt.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-workspace-deps-centralized",
    description: "Member crate pins a dependency instead of inheriting from the workspace.",
    remediation: "Declare the version once under `[workspace.dependencies]` in the \
                  root `Cargo.toml`, then reference it from member crates with \
                  `foo = { workspace = true }`. Keeps versions in lockstep and \
                  makes bumps a one-file change.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Toml, Backend::Text(Box::new(text::Check)))],
    }
}
