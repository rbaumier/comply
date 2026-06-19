mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-empty-object-keys",
    description: "Object key is empty.",
    remediation: "Remove this property or give it a meaningful key name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness"],

    // JSON parser test suites store fuzzing corpora and spec-conformance
    // fixtures full of intentional empty-string keys to exercise the grammar;
    // those `""` keys are the test input, not a production typo.
    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
