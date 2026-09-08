//! rust-no-errors-doc-section — the signature already documents the failures.
//!
//! A `# Errors` section restates what the reader has just read: the return
//! type names the error type, and that type's own docs (or its variants) say
//! what each failure means. What the section adds is a second, hand-written
//! copy of the same contract — one that drifts the moment a variant is added,
//! removed or renamed, because nothing checks it.
//!
//! In practice the section degenerates into a paraphrase of the signature
//! (`[CoreError] when a provider cannot be reached`), which teaches nothing and
//! costs a doc-comment edit on every change to the error type. The failure
//! modes belong in the error type: name the variants after the conditions, and
//! the contract is enforced by the compiler instead of by prose.
//!
//! `# Panics` and `# Safety` stay required by
//! [`rust-doc-sections-required`][crate::rules::rust_doc_sections_required]:
//! neither is visible in the signature, so only the doc can carry them.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-errors-doc-section",
    description: "Rustdoc carries no `# Errors` section — the return type documents the failures.",
    remediation: "Delete the `# Errors` section and its prose. The `Result<_, E>` \
                  in the signature already tells the caller the call can fail, and \
                  `E` tells them how — if it doesn't, name the error variants after \
                  the conditions they stand for instead of describing them in a \
                  doc comment nothing keeps in sync. Keep whatever the section said \
                  that the types cannot say (a caller-visible retry contract, for \
                  example) as ordinary prose in the summary.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
