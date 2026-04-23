//! zod-no-unknown-schema — `z.unknown()` opts out of validation.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-unknown-schema",
    description: "`z.unknown()` accepts anything — the schema provides no validation.",
    remediation: "Replace `z.unknown()` with a concrete schema that describes \
                  the expected shape (e.g. `z.object({...})`, `z.string()`, \
                  `z.array(...)`). If the value truly is unknown until runtime, \
                  validate it at the boundary where the shape becomes known.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
