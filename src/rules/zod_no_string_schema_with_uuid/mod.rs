//! zod-no-string-schema-with-uuid — prefer `z.uuid()` over `z.string().uuid()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-string-schema-with-uuid",
    description: "`z.string().uuid()` is deprecated in Zod v4 — use the top-level `z.uuid()` schema.",
    remediation: "Use z.uuid() instead of z.string().uuid() in Zod v4+",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
