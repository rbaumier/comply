//! no-loop-counter-reassign — flag reassignment of a C-style `for`
//! loop counter inside the loop body.
//!
//! ## Scope: TypeScript / JavaScript / TSX only.
//!
//! The rule targets the classic C-style `for (let i = 0; i < n; i++)`
//! pattern, where the loop declares a counter and the body is expected
//! to read but not reassign it. A bare `i = 5` inside that body breaks
//! the counted-iteration contract and almost always hides an off-by-one
//! error or a misplaced reset.
//!
//! ## Why no Rust backend
//!
//! Rust has no construct that matches this pattern. `for x in iter` is
//! a pattern binding over an iterator — the binding is immutable, and
//! the compiler rejects reassignment at the type level. `while cond`
//! and `loop` do not declare a counter at all; they run a boolean/
//! unconditional loop whose termination relies on body mutation, and
//! advancing a local variable (`i = end` to skip past a matched block,
//! `i += 1` otherwise) is the entire contract of those loops. Treating
//! any body assignment to a variable mentioned in the condition as a
//! "counter reassignment" — the previous Rust backend's heuristic —
//! flags idiomatic code like:
//!
//! ```rust,ignore
//! while i + 2 < len {
//!     if /* match `/**` block */ {
//!         i = end;    // advance past the matched block
//!     } else {
//!         i += 1;
//!     }
//! }
//! ```
//!
//! Sonar's analogous rule (`S127`) is explicitly for-loop-scoped and
//! there is no equivalent that makes sense for Rust `while`/`loop`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;

pub const META: RuleMeta = RuleMeta {
    id: "no-loop-counter-reassign",
    description: "Assignment to a `for` loop counter inside the loop body causes subtle bugs.",
    remediation: "Use a separate variable instead of reassigning the loop counter. Modifying the counter inside the body makes the loop hard to reason about and often hides off-by-one errors.",
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
