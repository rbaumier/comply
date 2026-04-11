//! no-incorrect-string-concat

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-incorrect-string-concat",
    description: "Suspicious string concatenation with a number variable.",
    remediation: "Use explicit conversion: `\"text\" + String(num)` or template literals: `\\`text${num}\\``.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
