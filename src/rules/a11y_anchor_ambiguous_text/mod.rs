//! a11y-anchor-ambiguous-text

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-anchor-ambiguous-text",
    description: "Flag `<a>` elements with ambiguous text like \"click here\" or \"read more\".",
    remediation: "Use descriptive link text that indicates the purpose of the link, e.g., \"View documentation\" instead of \"click here\".",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["accessibility"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
