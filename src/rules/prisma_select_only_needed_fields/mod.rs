//! prisma-select-only-needed-fields — `findMany` without `select`/`include`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-select-only-needed-fields",
    description: "`findMany` without `select` fetches every column — wasteful for wide tables.",
    remediation: "Add `select: { id: true, ... }` (or `include` for relations) to fetch only what's needed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
