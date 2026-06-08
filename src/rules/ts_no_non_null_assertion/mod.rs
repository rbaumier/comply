//! ts-no-non-null-assertion — disallow non-null assertions (`value!`).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-non-null-assertion",
    description: "Non-null assertions (`value!`) suppress compiler checks and can hide real nullability bugs.",
    remediation: "Narrow the type with a check (`if (value)`), use optional chaining (`value?.x`), or rework the types so the value is known to be non-null.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-non-null-assertion/"),
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
