//! no-undefined-assignment

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-undefined-assignment",
    description: "Assigning `undefined` explicitly is unnecessary.",
    remediation: "Use `let x;` instead of `let x = undefined;`, or use `delete obj.prop` instead of `obj.prop = undefined`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
