//! elysia-group-deep-paths

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-group-deep-paths",
    description: "Deep route paths repeated across handlers should be grouped via `.group()` or a `prefix`.",
    remediation: "Wrap deep paths under `.group('/v1/users', g => g.get('/profile', ...))` or pass `prefix` to a sub-app.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
