//! prisma-no-nested-create-without-connect — deeply nested create.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-nested-create-without-connect",
    description: "Nested `create` inside another `create` writes related rows that may become orphans on rollback.",
    remediation: "Use `connect: { id }` for existing relations and create children in a `$transaction`.",
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
