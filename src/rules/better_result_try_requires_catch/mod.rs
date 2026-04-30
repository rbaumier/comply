mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-result-try-requires-catch",
    description: "Result.try / Result.tryPromise must include both `try` and `catch`.",
    remediation: "Provide an object with both `try` and `catch` keys.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["better-result"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
