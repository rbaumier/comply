//! zod-string-min-1-required — bare `z.string()` accepts empty strings.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-string-min-1-required",
    description: "Bare `z.string()` without length constraints accepts empty strings.",
    remediation: "Add `.min(1)` or `.trim().min(1)` to reject empty strings.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
