//! elysia-numeric-body-no-coerce

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-numeric-body-no-coerce",
    description: "`t.Number()` inside a `body:` schema rejects numeric strings — use `t.Numeric()` for form-encoded payloads.",
    remediation: "Replace `t.Number()` with `t.Numeric()` in `body:` schemas so multipart/urlencoded numeric fields coerce.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["validation", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
