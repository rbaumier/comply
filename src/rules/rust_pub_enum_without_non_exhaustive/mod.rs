//! rust-pub-enum-without-non-exhaustive — pub enums need `#[non_exhaustive]`.
//!
//! Adding a new variant to a `pub enum` is a breaking change for
//! every downstream crate that pattern-matches on the enum without
//! a wildcard arm. `#[non_exhaustive]` flips that around: downstream
//! crates are forced from day one to write `_ => …`, so the next
//! variant you add doesn't break their build.
//!
//! This is a library hygiene rule. For binaries you don't ship as
//! a crate, the constraint doesn't apply — suppress with
//! `// comply-ignore` on the line above the enum.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-pub-enum-without-non-exhaustive",
    description: "`pub enum` without `#[non_exhaustive]` makes new variants a breaking change.",
    remediation: "Add `#[non_exhaustive]` above the enum. Downstream \
                  crates will need a wildcard `_ => …` arm to match it, \
                  which means future-you can add variants without \
                  releasing a major version.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
