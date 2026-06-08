//! testing-no-shared-state — flag top-level `let`/`var` in test files that
//! are mutated inside `test(...)` blocks without being reset in `beforeEach`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-shared-state",
    description: "Top-level let/var mutated across test() blocks without being reset in beforeEach — tests become order-dependent.",
    remediation: "Move the variable inside each test, or reset it in beforeEach(). Prefer fresh state per test over shared mutable state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],

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
