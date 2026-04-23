//! xstate-state-names

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-state-names",
    description: "State names inside `states: { ... }` must be camelCase or snake_case.",
    remediation: "Rename the state key so it starts with a lowercase letter and uses camelCase or snake_case (e.g. `idle`, `fetchingData`, `fetching_data`).",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/states"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
