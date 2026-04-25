//! zod-trim-before-min — `.min(1)` without `.trim()` accepts whitespace-only strings.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-trim-before-min",
    description: "`z.string().min(1)` without `.trim()` allows strings of only whitespace.",
    remediation: "Add `.trim()` before `.min(1)`: `z.string().trim().min(1)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
