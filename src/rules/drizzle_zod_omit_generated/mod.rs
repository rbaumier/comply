//! drizzle-zod-omit-generated

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-zod-omit-generated",
    description: "`createInsertSchema(table)` should `.omit({ id, createdAt, ... })` auto-generated columns.",
    remediation: "Chain `.omit({ id: true, createdAt: true, updatedAt: true })` on `createInsertSchema(...)` so API consumers don't submit DB-generated columns.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],

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
