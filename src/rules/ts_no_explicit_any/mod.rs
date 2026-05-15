//! ts-no-explicit-any — flag explicit `: any` and `as any`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-explicit-any",
    description: "Explicit `any` disables type checking — use `unknown` or a precise type.",
    remediation: "Replace `any` with `unknown` (when the value's shape is unknown — forces narrowing at use site), \
                  or with a precise type / generic when known.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-explicit-any/"),
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
