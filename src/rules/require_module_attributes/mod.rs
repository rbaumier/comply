//! require-module-attributes — flag imports/exports with empty `with {}`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "require-module-attributes",
    description: "Import/export with empty attribute list `with {}` is not allowed.",
    remediation: "Either add the required attributes (e.g. `with { type: 'json' }`) \
                  or remove the empty `with {}` clause.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
