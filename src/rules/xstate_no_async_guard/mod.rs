//! xstate-no-async-guard

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-async-guard",
    description: "XState `guard`/`cond` properties must be synchronous — async functions are not supported.",
    remediation: "Guards must be synchronous, use actors for async logic",
    severity: Severity::Error,
    doc_url: Some("https://stately.ai/docs/guards"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
