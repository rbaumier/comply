//! drizzle-junction-composite-pk

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-junction-composite-pk",
    description: "Junction tables with 2 FK columns must declare a composite `primaryKey`.",
    remediation: "Add `primaryKey({ columns: [t.aId, t.bId] })` in the table options callback so the junction table has a real composite primary key.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
