//! ui-no-wide-letter-spacing — inline `letterSpacing` above 0.05em hurts
//! readability for body copy and small UI text.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-wide-letter-spacing",
    description: "Inline `letterSpacing` above 0.05em — hurts readability.",
    remediation: "Keep `letterSpacing` at or below 0.05em for body text. Reserve wider tracking \
                  for short uppercase headings only.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
