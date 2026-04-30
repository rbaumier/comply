//! regex-sort-flags

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-sort-flags",
    description: "Regex flags should be alphabetically sorted for consistency (`dgimsvy`).",
    remediation: "Reorder the flags alphabetically: e.g. `/pattern/ig` → `/pattern/gi`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
