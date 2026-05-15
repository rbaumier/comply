//! ts-prefer-optional-chain — `a && a.b && a.b.c` → `a?.b?.c`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-optional-chain",
    description: "`a && a.b && a.b.c` is verbose and order-sensitive — `a?.b?.c` reads better and short-circuits the same way.",
    remediation: "Use optional chaining `?.` for property and call access on possibly-nullish values.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/prefer-optional-chain/"),
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
