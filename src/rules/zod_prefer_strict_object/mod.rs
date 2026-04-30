//! zod-prefer-strict-object — prefer `z.strictObject({...})` over `z.object({...}).strict()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-strict-object",
    description: "`z.object({...}).strict()` is deprecated in Zod v4 — the strictness \
                  is a top-level factory, not a chained modifier.",
    remediation: "Replace `z.object({...}).strict()` with `z.strictObject({...})`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
