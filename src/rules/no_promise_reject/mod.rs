//! no-promise-reject

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-promise-reject",
    description: "`Promise.reject()` makes error handling harder — prefer returning error values or throwing typed errors.",
    remediation: "Return a Result type, throw a typed error, or use `Promise.resolve()` with an error discriminant.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["functional"],

    // `Promise.reject(...)` is a production error-propagation anti-pattern: the
    // rule steers callers toward a Result / typed throw. In test code a rejected
    // promise is the *stimulus* — a `vi.fn(() => Promise.reject(...))` fixture
    // that drives a queryFn/mutation error branch under test — not error handling
    // to refactor, and there is no caller to hand a Result back to. Exempt the
    // test scope via the central gate (#5757), mirroring comply#1396.
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
        ],
    }
}
