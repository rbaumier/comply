//! rust-unsafe-impl-without-comment — `unsafe impl` needs a SAFETY comment.
//!
//! `unsafe impl Send for Foo {}` is a promise to the compiler that
//! every invariant required by `Send` holds. Without a comment
//! explaining WHICH invariants and HOW the type upholds them, no
//! reviewer (or future you) can audit the claim. The comment is
//! the entire audit trail for the unsafe contract.
//!
//! Sister rule to `rust-undocumented-unsafe`, but for trait impls
//! instead of expression blocks.

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-unsafe-impl-without-comment",
    description: "`unsafe impl` requires a `// SAFETY:` comment.",
    remediation: "Add a `// SAFETY: ...` comment immediately above the \
                  `unsafe impl` block. Spell out which invariants of \
                  the unsafe trait the type upholds — without it, the \
                  contract is unauditable.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
