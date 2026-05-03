//! drizzle-decimal-for-money

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-decimal-for-money",
    description: "`numeric('price')` / `decimal('amount')` for money columns must declare `precision`/`scale` — otherwise the underlying SQL type is unbounded.",
    remediation: "Pass `{ precision: ..., scale: ... }` (e.g. `numeric('price', { precision: 12, scale: 2 })`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "drizzle", "database"],
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
