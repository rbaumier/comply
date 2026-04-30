//! xstate-no-imperative-action

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-imperative-action",
    description: "`send()` / `raise()` must only be called inside an action context.",
    remediation: "Wrap the call in an action (e.g. `actions: [send(...)]` or inside an action function), not at top level or in module scope.",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/actions"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
