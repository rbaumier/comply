//! elysia-drizzle-intermediate-var

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-drizzle-intermediate-var",
    description: "Inline `t.Omit(createInsertSchema(...))` triggers `Type instantiation is possibly infinite`.",
    remediation: "Bind `createInsertSchema(table)` to a variable first, then call `t.Omit(schema, [...])` on it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
