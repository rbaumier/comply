//! no-redundant-jump

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-jump",
    description: "Redundant `return;` at end of function or `continue;` at end of loop body.",
    remediation:
        "Remove the redundant `return;` or `continue;` — execution already falls through naturally.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
