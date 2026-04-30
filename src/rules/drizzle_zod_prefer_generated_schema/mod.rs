mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-zod-prefer-generated-schema",
    description: "Manual `z.object({})` in a Drizzle schema file duplicates column definitions.",
    remediation: "Use `createInsertSchema`/`createSelectSchema` from `drizzle-zod` to generate Zod schemas from the table definition.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/zod"),
    categories: &["drizzle", "zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
