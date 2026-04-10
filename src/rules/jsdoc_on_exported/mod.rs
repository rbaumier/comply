//! jsdoc-on-exported — every exported function needs a JSDoc block.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-on-exported",
    description: "Exported functions must document their public contract.",
    remediation: "Add a `/** ... */` JSDoc block above the export, \
                  describing what the function does, its parameters, and \
                  what it returns. Include an @example when the call site \
                  isn't obvious.",
    severity: Severity::Warning,
    doc_url: None,
};pub fn register() -> RuleDef {
    crate::register_ts_family_with_clippy_marker!(META, typescript, "missing_docs")
}
