//! xstate-no-invalid-conditional-action

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "xstate-no-invalid-conditional-action",
    description: "XState `choose(...)` branches must declare both a `guard`/`cond` and `actions` property.",
    remediation: "choose() branches must have guard/cond and actions properties",
    severity: Severity::Warning,
    doc_url: Some("https://stately.ai/docs/actions#choose-action"),
    categories: &["xstate"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
