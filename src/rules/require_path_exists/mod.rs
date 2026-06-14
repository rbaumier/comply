//! require-path-exists

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "require-path-exists",
    description: "Relative imports must point to files that exist.",
    remediation: "Fix the import path or create the missing file.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["imports"],

    skip_in_test_dir: false,
    // Fixture/sample directories (`fixtures/`, `__fixtures__/`, `samples/`, …)
    // hold linter/parser test-input files that intentionally contain imports to
    // non-existent modules as test data, never resolved as real modules, so the
    // rule does not apply there.
    skip_in_relaxed_dir: true,
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
