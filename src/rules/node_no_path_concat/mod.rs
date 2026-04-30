//! node-no-path-concat

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-no-path-concat",
    description: "String concatenation with `__dirname` / `__filename` is platform-dependent.",
    remediation: "Use `path.join()` or `path.resolve()` instead of string concatenation.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
