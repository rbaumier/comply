//! zod-prefer-safe-parse — route handlers should not let `ZodError` escape.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-safe-parse",
    description: "`.parse()` in a route handler throws `ZodError` unhandled — use `.safeParse()` instead.",
    remediation: "Use `.safeParse()` and handle `!result.success` to return a structured 400 response.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod", "api"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
