//! no-absolute-path

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-absolute-path",
    description: "Import uses an absolute path — use relative or aliased paths.",
    remediation: "Replace the absolute path import with a relative path (`./…`) or a configured path alias (`@/…`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
