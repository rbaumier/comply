//! elysia-numeric-no-bounds

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-numeric-no-bounds",
    description: "`t.Number()` / `t.Numeric()` is declared without `minimum` or `maximum` bounds.",
    remediation: "Add at least `{ minimum: 1 }` (IDs) or `{ minimum: 0, maximum: 100 }` (percentages) so the schema rejects out-of-range values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
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
