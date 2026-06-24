//! pure-by-default
//!
//! Test files are skipped (`skip_in_test_dir`). The rule's whole point is
//! testability: a function reading top-level mutable state should take that
//! state as a parameter so it can be exercised in isolation. Inside a test
//! file that rationale is moot — the function *is* the test — and the
//! module-level mutable binding it reads is the deliberate test-harness seam
//! (a `let mockSearch`/`navigateMock` that a `vi.mock` factory closes over).
//! A `vi.mock` factory is invoked by the test runner with no caller, so the
//! seam structurally cannot be passed as a parameter, and flagging it is a
//! false positive (issue #2240).

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "pure-by-default",
    description: "Function references top-level mutable state.",
    remediation: "Pass the state as a parameter instead of referencing a top-level `let`/`var`. This makes the function pure and easier to test.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
