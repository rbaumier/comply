//! drizzle-prefer-infer-select

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-prefer-infer-select",
    description: "Prefer `typeof table.$inferSelect` over `InferSelectModel<typeof table>`.",
    remediation: "Replace `InferSelectModel<typeof table>` with `typeof table.$inferSelect` (and `InferInsertModel` with `$inferInsert`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
