//! ui-exit-duration-shorter-enter — exit animations should be at most as
//! long as the enter animation so dismiss feels responsive.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-exit-duration-shorter-enter",
    description: "The `exit` transition of a `motion.*` component should be at most as long as the enter transition.",
    remediation: "Lower the exit `duration` (or raise the enter `duration`) so the element dismisses as fast as or faster than it appears.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
