mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-inline-implementation",
    description: "Inline functions as XState `actions`, `guards`, or `services` hinder reuse and testing.",
    remediation: "Use named actions/guards/services instead of inline functions",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/machines#implementations"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
