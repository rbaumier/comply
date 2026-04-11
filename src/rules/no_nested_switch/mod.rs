//! no-nested-switch

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-nested-switch",
    description: "`switch` inside another `switch` is hard to follow.",
    remediation: "Extract the inner switch into a separate function. Nested switches create deeply indented, hard-to-read code that is easy to get wrong.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
