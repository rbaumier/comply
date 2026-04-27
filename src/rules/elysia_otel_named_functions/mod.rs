//! elysia-otel-named-functions

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-otel-named-functions",
    description: "Anonymous arrow functions in `.derive` / `.resolve` produce unnamed OpenTelemetry spans, gutting trace readability.",
    remediation: "Pass a named function (e.g. `function deriveUser({ ... }) { ... }`) so OTEL emits a meaningful span name.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["observability", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
