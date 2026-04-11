//! no-confidential-logging

//! no-confidential-logging

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-confidential-logging",
    description: "Logging calls must not contain sensitive data such as passwords, tokens, or API keys.",
    remediation: "Remove or redact sensitive values before logging. Use structured logging with explicit field allow-lists instead of interpolating raw secrets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
