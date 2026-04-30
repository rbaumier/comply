//! no-small-switch

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-small-switch",
    description: "`switch` with fewer than 3 cases — use `if/else` instead.",
    remediation: "Replace small `switch` statements (< 3 cases) with `if/else` chains. `switch` adds indentation and boilerplate (`break`, `case`, `default`) that isn't justified for 1-2 branches.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
