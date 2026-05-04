//! drizzle-prefer-inarray

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prefer-inarray",
    description: "Prefer `inArray(col, [...])` over `sql` templates with `IN (...)`.",
    remediation: "Replace `sql`… `IN (...)` with `inArray(col, [...])` — Drizzle's helper is parameterized and safer.",
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
