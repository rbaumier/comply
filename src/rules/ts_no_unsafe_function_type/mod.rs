//! ts-no-unsafe-function-type — flag `: Function` type (loses signature info).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-unsafe-function-type",
    description: "The built-in `Function` type is unsafe — it accepts any callable and loses signature information.",
    remediation: "Replace `Function` with a precise function signature like `(arg: T) => U` or `() => void`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-unsafe-function-type/"),
    categories: &["typescript"],
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
