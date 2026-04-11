//! no-put-method — prefer PATCH over PUT for updates.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-put-method",
    description: "PUT replaces the entire resource; PATCH updates fields.",
    remediation: "Replace `method: 'PUT'` with `method: 'PATCH'` for \
                  partial updates. PUT requires you to send every field \
                  every time; PATCH accepts only the fields you want to \
                  change. Use PUT only when you genuinely want full \
                  replacement semantics, and comment why.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
