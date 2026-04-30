//! useless-string-operation

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "useless-string-operation",
    description: "String method result is ignored \u{2014} strings are immutable.",
    remediation: "Assign the result: `str = str.trim()`. String methods return a new value and never mutate in place.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
