//! no-empty-file

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-empty-file",
    description: "Empty files are not allowed — they add noise without value.",
    remediation: "Add meaningful content or delete the file.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

    // Empty files inside test directories are intentional: ESLint-plugin test
    // suites use empty fixtures to verify rules handle no-content files, and as
    // stub files referenced by name. The test-dir signal already covers
    // `fixtures/`, `__tests__/`, `/tests/`, etc.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Rust,
        Language::Vue,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    RuleDef {
        meta: META,
        backends,
    }
}
