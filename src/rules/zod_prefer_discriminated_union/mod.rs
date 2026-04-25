//! zod-prefer-discriminated-union — prefer `z.discriminatedUnion` when
//! the union branches share a literal tag field.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-discriminated-union",
    description: "`z.union([z.object({...}), ...])` with shared discriminant fields should use `z.discriminatedUnion()`.",
    remediation: "Use `z.discriminatedUnion('type', [...])` for faster parsing and better error messages.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
