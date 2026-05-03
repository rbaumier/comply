//! drizzle-prepared-placeholder

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prepared-placeholder",
    description: "`prepare()` chains must use `sql.placeholder(...)` in `where`, not inline variables.",
    remediation: "Replace inline variables inside `.where(...)` of a `.prepare()` chain with `sql.placeholder('name')` and bind values at execution time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
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
