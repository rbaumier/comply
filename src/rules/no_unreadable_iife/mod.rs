//! no-unreadable-iife — flag unreadable chained IIFE calls.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unreadable-iife",
    description: "IIFE with parenthesized arrow function body is unreadable.",
    remediation: "Extract the inner expression from the arrow function body \
                  into a variable, or remove the unnecessary parentheses \
                  around the body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
