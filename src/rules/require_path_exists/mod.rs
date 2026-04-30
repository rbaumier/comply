//! require-path-exists

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "require-path-exists",
    description: "Relative imports must point to files that exist.",
    remediation: "Fix the import path or create the missing file.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
