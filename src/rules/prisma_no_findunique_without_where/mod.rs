//! prisma-no-findunique-without-where — `findUnique` without `where` returns null.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-findunique-without-where",
    description: "`findUnique` without a `where` argument always resolves to null.",
    remediation: "Pass `{ where: { id } }`, or switch to `findFirst` if filtering on a non-unique field.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
