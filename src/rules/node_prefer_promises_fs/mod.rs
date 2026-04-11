//! node-prefer-promises-fs

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "node-prefer-promises-fs",
    description: "Callback-based `fs.*` methods are discouraged.",
    remediation: "Use `fs.promises.*` or import from `fs/promises` instead of callback-based `fs` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["node"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
