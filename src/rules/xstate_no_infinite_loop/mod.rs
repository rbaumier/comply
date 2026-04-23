//! xstate-no-infinite-loop

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-infinite-loop",
    description: "XState `always` transitions without a guard that stay in (or re-target) the same state cause infinite evaluation loops.",
    remediation: "Add guard to always transition or target different state",
    severity: Severity::Error,
    doc_url: Some("https://stately.ai/docs/eventless-transitions"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
