//! regex-anchor-precedence

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "regex-anchor-precedence",
    description: "Anchor `^` or `$` in alternation may not bind as expected.",
    remediation: "Wrap the alternation in a group: `/^(a|b)$/` instead of `/^a|b$/`. Without grouping, `/^a|b$/` means `(^a)|(b$)`, not `^(a|b)$`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality", "regex"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
