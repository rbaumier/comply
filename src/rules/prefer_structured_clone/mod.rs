//! prefer-structured-clone — prefer `structuredClone()` over `JSON.parse(JSON.stringify())`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-structured-clone",
    description: "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` for deep cloning.",
    remediation: "Replace `JSON.parse(JSON.stringify(x))` with `structuredClone(x)`. \
                  `structuredClone` handles circular references, typed arrays, and \
                  other values that JSON serialization silently drops or corrupts.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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
