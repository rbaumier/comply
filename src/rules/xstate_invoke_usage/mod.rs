//! xstate-invoke-usage

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-invoke-usage",
    description: "`invoke` must be an object (or array of objects) with at least a `src` property.",
    remediation: "Add `src` to the invoke object. Optional keys: `onDone`, `onError`, `id`, `input`, `systemId`.",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/invoke"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
