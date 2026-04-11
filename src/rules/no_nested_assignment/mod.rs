//! no-nested-assignment

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-nested-assignment",
    description: "Assignment inside a condition or sub-expression is likely a bug.",
    remediation: "Move the assignment before the condition: `x = value; if (x) { ... }`. If intentional, use a separate statement.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
