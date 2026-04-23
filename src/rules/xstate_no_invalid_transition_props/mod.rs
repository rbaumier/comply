//! xstate-no-invalid-transition-props

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-invalid-transition-props",
    description: "Transition objects in XState `on` handlers must only use known properties.",
    remediation: "Use only valid XState transition properties",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/transitions"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
