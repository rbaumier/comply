//! no-collapsible-if

//! no-collapsible-if

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-collapsible-if",
    description: "Nested `if` can be merged with `&&`.",
    remediation: "Merge `if (a) { if (b) { ... } }` into `if (a && b) { ... }`. Unnecessary nesting wastes indent levels.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
