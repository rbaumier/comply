//! prefer-set-has — flag array `.includes()` inside loops -> use `Set#has()`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-has",
    description: "Prefer `Set#has()` over `Array#includes()` when checking for existence or non-existence.",
    remediation: "Convert the array to a `Set` and use `.has()` instead of \
                  `.includes()`. `Array#includes()` is O(n) per call; \
                  `Set#has()` is O(1). This matters when the check is inside \
                  a loop or called repeatedly.",
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
