//! regex-no-multiple-spaces

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-multiple-spaces",
    description: "Multiple consecutive spaces in regex are hard to read and count.",
    remediation: "Use a quantifier: ` {2}` or `\\s{2,}` instead of multiple spaces.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
