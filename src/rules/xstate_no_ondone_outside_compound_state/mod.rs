//! xstate-no-ondone-outside-compound-state

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-ondone-outside-compound-state",
    description: "XState `onDone` is only valid on compound states (with nested `states`) or invoking states (with `invoke`).",
    remediation: "onDone only valid on compound states (with states) or invoking states (with invoke)",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/final-states"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
