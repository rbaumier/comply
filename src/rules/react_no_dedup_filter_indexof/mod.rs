//! react-no-dedup-filter-indexof — `arr.filter((v, i, a) => a.indexOf(v) === i)`.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-dedup-filter-indexof",
    description: "Deduping via `filter((v, i, a) => a.indexOf(v) === i)` is O(n²).",
    remediation: "Use `[...new Set(arr)]` — O(n).",
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
