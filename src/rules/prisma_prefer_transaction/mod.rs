//! prisma-prefer-transaction — multiple writes in one function need `$transaction`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-prefer-transaction",
    description: "Two or more Prisma write calls in the same function should run in `$transaction`.",
    remediation: "Wrap the writes in `prisma.$transaction([...])` so they commit/rollback atomically.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma"],
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
