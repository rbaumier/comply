//! rust-lib-without-missing-docs — a library crate root that exposes public API
//! without ever setting a level for the `missing_docs` lint.
//!
//! `missing_docs` is `allow` by default, so an undocumented `pub` item ships in
//! silence: nothing in the build tells the author, and the gap only surfaces on
//! docs.rs once consumers are already reading it. Declaring the lint once at the
//! crate root — `#![deny(missing_docs)]` in `src/lib.rs`, or `missing_docs`
//! under `[lints.rust]` in `Cargo.toml` — turns every later undocumented
//! addition into a build signal.
//!
//! The rule fires on `lib.rs` only, and only when the crate root actually
//! exposes something public and no `missing_docs` level is declared anywhere the
//! crate can reach.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-lib-without-missing-docs",
    description: "A library crate root exposes public API with no `missing_docs` lint level — undocumented `pub` items ship without a single warning.",
    remediation: "Add `#![deny(missing_docs)]` (or `#![warn(missing_docs)]` while catching up) at the top of `src/lib.rs`, or declare `missing_docs = \"warn\"` under `[lints.rust]` in the crate's `Cargo.toml`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
