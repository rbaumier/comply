//! for-loop-increment-sign

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "for-loop-increment-sign",
    description: "For-loop increment goes the wrong direction relative to the condition.",
    remediation: "Fix the increment direction: use `i++` with `i <` conditions and `i--` with `i >` conditions.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
