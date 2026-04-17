//! regex-prefer-predefined-assertion

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-predefined-assertion",
    description: "Lookaround assertion can be replaced with a simpler predefined assertion like `\\b` or `^`/`$`.",
    remediation: "Replace the lookaround with the equivalent predefined assertion.",
    severity: Severity::Warning,
    doc_url: Some("https://ota-meshi.github.io/eslint-plugin-regexp/rules/prefer-predefined-assertion.html"),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
