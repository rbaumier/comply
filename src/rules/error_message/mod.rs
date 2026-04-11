//! error-message — enforce passing a message to built-in Error constructors.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "error-message",
    description: "Pass a message to the Error constructor.",
    remediation: "Add a descriptive string message as the first argument to the Error \
                  constructor (or second for AggregateError, third for SuppressedError). \
                  Empty strings and non-string literals are also flagged.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
