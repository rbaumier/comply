//! require-module-specifiers — flag import/export with empty specifiers.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "require-module-specifiers",
    description: "Import/export statements with empty specifier lists are not allowed.",
    remediation: "Add specifiers to the import/export, convert to a side-effect \
                  import, or remove the statement entirely.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
