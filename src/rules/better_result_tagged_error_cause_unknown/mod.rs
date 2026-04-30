mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-tagged-error-cause-unknown",
    description: "The cause field in TaggedError must be typed `unknown`, not Error/any.",
    remediation: "Declare `cause: unknown` so callers can't rely on a specific error shape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
