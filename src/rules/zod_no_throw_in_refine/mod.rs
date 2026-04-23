//! zod-no-throw-in-refine — throwing inside `.refine()` / `.superRefine()`
//! callbacks bypasses Zod's issue aggregation.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-throw-in-refine",
    description: "`throw` inside `.refine()` / `.superRefine()` bypasses Zod's issue aggregation and surfaces as an unhandled exception instead of a validation error.",
    remediation: "Use ctx.addIssue() in superRefine, or return false in refine",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
