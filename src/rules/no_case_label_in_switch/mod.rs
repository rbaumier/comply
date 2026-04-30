//! no-case-label-in-switch

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-case-label-in-switch",
    description: "Label statement inside switch looks like a case but is a JS label.",
    remediation: "Use `case <value>:` instead. A bare `identifier:` inside a switch is a label statement, not a case branch.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
