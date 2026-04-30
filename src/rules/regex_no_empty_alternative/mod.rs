//! regex-no-empty-alternative

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-alternative",
    description: "Empty alternative in regex matches empty string and is likely a mistake.",
    remediation: "Remove the leading, trailing, or consecutive `|` in the regex pattern.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
