//! regex-no-extra-lookaround-assertions

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-extra-lookaround-assertions",
    description: "Lookaround assertion is useless and can be inlined into the parent pattern.",
    remediation: "Remove the unnecessary lookaround wrapper and inline its contents.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-extra-lookaround-assertions.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
