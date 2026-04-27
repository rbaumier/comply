//! prisma-no-delete-without-where — `deleteMany()` without where wipes the table.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-delete-without-where",
    description: "`deleteMany()` without `where` deletes every row in the table.",
    remediation: "Add `where: { ... }`. If you really mean to wipe the table, do it from a maintenance script.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["prisma", "safety"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
