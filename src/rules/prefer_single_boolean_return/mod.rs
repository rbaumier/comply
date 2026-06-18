//! prefer-single-boolean-return

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-single-boolean-return",
    description: "`if (cond) return true; else return false;` can be replaced by `return cond;`.",
    remediation: "Return the condition (or its negation) directly: `return cond;` or `return !cond;`.",
    severity: Severity::Warning,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S1126"),
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
        ],
    }
}
