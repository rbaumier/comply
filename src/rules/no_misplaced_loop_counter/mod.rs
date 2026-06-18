//! no-misplaced-loop-counter — flag C-style `for` loops whose update
//! clause modifies a different variable than the condition tests.
//!
//! ## Scope: TypeScript / JavaScript / TSX only.
//!
//! The legitimate target is `for (let i = 0; i < n; j++)` — a
//! copy-paste bug where the update clause was left pointing at a
//! stale variable. The backend pulls the condition variable from the
//! `binary_expression` on the `condition` field and the update
//! variable from an `update_expression` / `augmented_assignment`
//! on the `increment` field, and reports the mismatch.
//!
//! ## Why no Rust backend
//!
//! A `while cond { body }` or `loop { body }` has no update clause
//! separate from the condition. The body mutates whatever the loop
//! needs, and composite-state loops (`count` + `p` scanning backwards
//! for escaped characters, `lo` + `hi` + `mid` in binary search,
//! `i` + `j` in two-pointer algorithms) mutate multiple variables by
//! design. The previous Rust backend extracted the first identifier
//! from the condition text, scanned the body for the first `+= 1`,
//! and flagged any mismatch, which misfires on exactly this class of
//! code:
//!
//! ```rust,ignore
//! while p > 0 && bytes[p - 1] == b'\\' {
//!     count += 1;    // was flagged as "the update"
//!     p -= 1;        // the real advance, not seen by the scanner
//! }
//! ```
//!
//! Sonar's for-loop counter rules (`S1994` and neighbours) are
//! explicitly for-loop-scoped, because the idea of a "misplaced
//! update" only makes sense when the update clause is syntactically
//! distinct from the body. Same call as entries #17 and #25.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-misplaced-loop-counter",
    description: "`for` loop update clause modifies a different variable than the condition.",
    remediation: "Ensure the update expression (`i++`) modifies the same variable used in the loop condition (`i < n`). Mismatched variables usually indicate a copy-paste bug.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
