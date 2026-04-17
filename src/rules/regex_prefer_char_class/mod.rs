//! regex-prefer-char-class

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-prefer-char-class",
    description: "Single-character alternations should use a character class.",
    remediation: "Replace `a|b|c` with `[abc]`. Character classes are more readable and often faster than alternation for single characters.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
