//! regex-no-empty-character-class

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-no-empty-character-class",
    description: "Empty character class `[]` matches nothing and is likely a mistake.",
    remediation: "Remove the empty `[]` or add characters inside the brackets.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
