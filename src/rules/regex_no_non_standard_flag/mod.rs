//! regex-no-non-standard-flag

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-non-standard-flag",
    description: "Regex uses a non-standard flag that is not part of the ECMAScript specification.",
    remediation: "Remove the non-standard flag. Standard flags are: d, g, i, m, s, u, v, y.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-non-standard-flag.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
