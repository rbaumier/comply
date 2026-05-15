//! prisma-soft-delete-filter — `findMany` / `findFirst` requires `deletedAt: null` filter.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-soft-delete-filter",
    description: "`prisma.<model>.findMany() / findFirst()` without a `deletedAt: null` (or equivalent) filter returns soft-deleted rows.",
    remediation: "Add `where: { deletedAt: null, ... }` (or your project's soft-delete predicate) so soft-deleted rows don't leak into the result set.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma", "safety"],
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
