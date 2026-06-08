//! ts-ban-ts-comment — disallow `@ts-<directive>` comments or require descriptions.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-ban-ts-comment",
    description: "`@ts-ignore` and `@ts-nocheck` suppress compiler errors and hide bugs.",
    remediation: "Fix the underlying type error, or use `@ts-expect-error` with a description.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/ban-ts-comment/"),
    categories: &["typescript"],

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
