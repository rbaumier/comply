//! ts-prefer-as-const — `as "literal"` / `as 42` should be `as const`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-as-const",
    description: "Casting to a literal type (`as \"foo\"`, `as 42`) loses the link to the value — use `as const` instead.",
    remediation: "Replace `value as \"literal\"` with `value as const`. The const assertion preserves the literal type without forcing the cast.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-as-const/"),
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
