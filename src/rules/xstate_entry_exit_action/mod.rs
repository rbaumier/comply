//! xstate-entry-exit-action

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-entry-exit-action",
    description: "`entry` and `exit` must be a string, a function, or an array of those.",
    remediation: "Use `entry: 'actionName'`, `entry: () => {}`, or `entry: ['a', 'b']`. Do not pass a plain object.",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/actions#entry-and-exit-actions"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
