//! prefer-destructuring-assignment

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-destructuring-assignment",
    description: "Consecutive property accesses on the same object can be destructured.",
    remediation: "Use destructuring: `const { x, y } = obj;` instead of separate `const x = obj.x; const y = obj.y;` declarations. Destructuring is more concise and makes the intent clear.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
