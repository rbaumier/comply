//! zod-no-manual-types — prefer `z.infer<typeof Schema>` over hand-rolled types
//! that mirror a Zod schema in the same file.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-manual-types",
    description: "A hand-written `type` that duplicates the keys of a nearby \
                  `z.object({...})` will drift from the schema and defeat runtime \
                  validation guarantees.",
    remediation: "Derive the type with `type T = z.infer<typeof Schema>` so the \
                  type always matches the schema.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
