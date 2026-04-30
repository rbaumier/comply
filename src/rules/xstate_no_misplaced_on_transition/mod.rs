//! xstate-no-misplaced-on-transition

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-misplaced-on-transition",
    description: "XState `on` must live on state nodes, not inside `invoke` or directly under `states`.",
    remediation: "on property must be on state nodes, not inside invoke or states object directly",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/transitions"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
