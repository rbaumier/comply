//! node-handle-callback-err

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "node-handle-callback-err",
    description: "Callback error parameter is declared but never used.",
    remediation: "Handle the error parameter (log it, rethrow, or forward). If intentionally unused, prefix with `_`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
