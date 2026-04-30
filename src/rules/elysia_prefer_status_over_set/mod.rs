//! elysia-prefer-status-over-set

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-prefer-status-over-set",
    description: "`set.status = code` mutates context — use the typed `status(code, body)` helper instead.",
    remediation: "Use `status(code, body)` instead of `set.status = code` for type-safe responses.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
