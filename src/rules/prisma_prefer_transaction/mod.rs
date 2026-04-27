//! prisma-prefer-transaction — multiple writes in one function need `$transaction`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-prefer-transaction",
    description: "Two or more Prisma write calls in the same function should run in `$transaction`.",
    remediation: "Wrap the writes in `prisma.$transaction([...])` so they commit/rollback atomically.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
