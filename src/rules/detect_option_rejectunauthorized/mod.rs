//! detect-option-rejectunauthorized

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "detect-option-rejectunauthorized",
    description: "`rejectUnauthorized: false` disables TLS certificate validation.",
    remediation: "Remove the option (it defaults to `true`) or set it to `true`. Disabling certificate validation makes the connection vulnerable to MITM attacks.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
