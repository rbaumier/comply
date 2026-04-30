//! ts-no-restricted-types — disallow certain types from being used.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-restricted-types",
    description: "Certain types are banned by project convention or because better alternatives exist.",
    remediation: "Replace the restricted type with the recommended alternative.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-restricted-types"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
