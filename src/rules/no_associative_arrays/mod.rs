//! no-associative-arrays

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-associative-arrays",
    description: "Arrays should not be used as associative arrays (use Map or object instead).",
    remediation: "Use `Map<string, T>` or a plain object `Record<string, T>` instead of assigning string keys on an array.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
