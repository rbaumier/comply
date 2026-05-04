//! react-no-sort-for-extrema — `.sort(...)[0]` / `sorted[length - 1]` for min/max.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-sort-for-extrema",
    description: "Sorting an array to pick only its first or last element is O(n log n) \
                  for work that can be done in O(n).",
    remediation: "Use a single-pass `Math.min` / `Math.max` or a manual fold.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "code-quality"],
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
