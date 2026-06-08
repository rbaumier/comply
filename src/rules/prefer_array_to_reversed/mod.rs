//! Prefer `toReversed()` over `[...arr].reverse()` (ES2023).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-to-reversed",
    description: "Prefer `arr.toReversed()` over `[...arr].reverse()`.",
    remediation: "Replace `[...arr].reverse()` or `arr.slice().reverse()` with `arr.toReversed()` (ES2023).",
    severity: Severity::Warning,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/toReversed",
    ),
    categories: &["e18e", "modernization"],

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
