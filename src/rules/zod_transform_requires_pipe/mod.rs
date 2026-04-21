//! zod-transform-requires-pipe — `.transform()` returns `z.any()` in
//! terms of the parser output. Without a following `.pipe(z.*)` the
//! schema silently produces an un-validated value. Requiring `.pipe()`
//! forces authors to re-assert the output type at the boundary.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-transform-requires-pipe",
    description: "`.transform()` returns an untyped value — follow with `.pipe(z.*)` to re-validate.",
    remediation: "Chain `.pipe(z.string())` (or the appropriate schema) after `.transform()` so the transformed value is validated.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
