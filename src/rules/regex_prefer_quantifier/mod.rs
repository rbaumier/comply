//! regex-prefer-quantifier

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-quantifier",
    description: "Repeated identical characters or escape sequences in regex should use quantifiers.",
    remediation: "Use quantifiers: `aaa` -> `a{3}`, `\\d\\d\\d\\d` -> `\\d{4}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
