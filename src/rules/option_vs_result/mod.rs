//! option-vs-result

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "option-vs-result",
    description: "Functions named `find*`/`get*` returning `null`/`undefined` should use an Option type.",
    remediation: "Wrap the return value in an Option/Result type instead of returning bare `null` or `undefined`. This makes the absence of a value explicit in the type system.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
