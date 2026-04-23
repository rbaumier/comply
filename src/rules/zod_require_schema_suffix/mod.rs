//! zod-require-schema-suffix — exported Zod schemas should end in `Schema`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-require-schema-suffix",
    description: "Exported Zod schemas should be named with a `Schema` suffix.",
    remediation: "Rename the export so the name ends in `Schema` (e.g. \
                  `export const UserSchema = z.object({...})`). The naming \
                  convention keeps the schema distinguishable from the \
                  inferred TypeScript type (`type User = z.infer<typeof UserSchema>`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
