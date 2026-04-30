//! prisma-require-transaction-for-multi-write — atomicity guard.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-require-transaction-for-multi-write",
    description: "Multiple Prisma write operations in the same file without `$transaction` — partial failures leave inconsistent state.",
    remediation: "Wrap the writes in `prisma.$transaction([...])` (sequential) or `prisma.$transaction(async (tx) => { ... })` (interactive).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma", "safety"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
