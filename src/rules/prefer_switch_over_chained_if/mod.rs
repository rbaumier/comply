//! prefer-switch-over-chained-if — 4+ if/else-if on one discriminant.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-switch-over-chained-if",
    description: "Long if/else-if chains should be switch statements.",
    remediation: "Convert a 4+ branch if/else-if chain into a `switch` \
                  statement. Switch makes the discriminant obvious and \
                  lets TypeScript warn on missing cases for union types.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

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
            (Language::Rust, Backend::Clippy { lint: "clippy::comparison_chain" }),
        ],
    }
}
