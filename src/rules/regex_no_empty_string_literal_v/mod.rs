//! regex-no-empty-string-literal-v

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-string-literal-v",
    description: "Empty string disjunction in a `v`-flag character class is unexpected and likely a mistake.",
    remediation: "Remove the empty string literal from the character class string disjunction.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://ota-meshi.github.io/eslint-plugin-regexp/rules/no-empty-string-literal.html",
    ),
    categories: &["regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
