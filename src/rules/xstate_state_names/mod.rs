//! xstate-state-names

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-state-names",
    description: "State names inside `states: { ... }` must be camelCase or snake_case.",
    remediation: "Rename the state key so it starts with a lowercase letter and uses camelCase or snake_case (e.g. `idle`, `fetchingData`, `fetching_data`).",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/states"),
    categories: &["xstate"],

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
