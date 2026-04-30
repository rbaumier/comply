//! no-done-callback — flag `test`/`it` callbacks that take a `done`
//! parameter (legacy async style). Prefer async/await.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-done-callback",
    description: "Test callbacks that take a `done` parameter use the legacy async style.",
    remediation: "Use async/await instead of done callback.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
