//! function-inside-loop

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "function-inside-loop",
    description: "Function declaration or expression inside a loop.",
    remediation: "Move the function outside the loop, or use an arrow function if a closure over the loop variable is intended. Declaring functions inside loops creates a new function object on every iteration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
