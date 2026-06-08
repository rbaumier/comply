//! drizzle-soft-delete-filter

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-soft-delete-filter",
    description: "Queries on soft-deletable tables must filter `isNull(t.deletedAt)`.",
    remediation: "Add `isNull(t.deletedAt)` (or equivalent) in the `where` clause of `select()` / `findMany()` calls in modules that reference `deletedAt`.",
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
