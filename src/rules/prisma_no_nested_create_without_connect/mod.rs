//! prisma-no-nested-create-without-connect — deeply nested create.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-nested-create-without-connect",
    description: "Nested `create` inside another `create` writes related rows that may become orphans on rollback.",
    remediation: "Use `connect: { id }` for existing relations and create children in a `$transaction`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
