mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "import-no-unresolved",
    description: "Relative import path must resolve to an existing file.",
    remediation: "Fix the import path — the target file may have been moved, renamed, or deleted.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/import-js/eslint-plugin-import/blob/main/docs/rules/no-unresolved.md",
    ),
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
