//! ts-default-param-last — enforce default parameters to be last.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-default-param-last",
    description: "Default parameters should be last to allow callers to omit them positionally.",
    remediation: "Move parameters with default values to the end of the parameter list.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/default-param-last/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
