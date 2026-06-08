//! react-no-chained-filter-map-reduce — 3+ chained `.filter/.map/.reduce` calls.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-chained-filter-map-reduce",
    description: "Three or more consecutive `.filter`/`.map`/`.reduce` calls walk the array \
                  multiple times and allocate intermediate arrays.",
    remediation: "Collapse the chain into a single `for`/`reduce` pass or use a lazy iterator.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "code-quality"],

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
