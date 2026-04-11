//! ts-no-use-before-define — disallow use of variables before they are defined.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-use-before-define",
    description: "Using variables before their definition leads to confusing code and potential TDZ errors.",
    remediation: "Move the declaration before its first usage, or restructure the code.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-use-before-define"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
