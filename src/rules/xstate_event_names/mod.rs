//! xstate-event-names

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-event-names",
    description: "XState event names must be SCREAMING_SNAKE_CASE.",
    remediation: "Rename the event key to uppercase letters, digits, and underscores (e.g. `NEXT`, `FETCH_DATA`).",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/events"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
