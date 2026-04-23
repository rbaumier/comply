mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-invalid-state-props",
    description: "Unknown property on an XState state node — likely a typo or misplaced config.",
    remediation: "Use only valid XState state node properties",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/state-nodes"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
