//! zod-prefer-loose-object — prefer `z.looseObject({...})` over `z.object({...}).passthrough()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-loose-object",
    description: "`z.object({...}).passthrough()` is replaced by the top-level \
                  `z.looseObject(...)` factory in Zod v4.",
    remediation: "Replace `z.object({...}).passthrough()` with `z.looseObject({...})`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
