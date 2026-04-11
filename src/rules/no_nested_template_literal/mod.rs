//! no-nested-template-literal

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-nested-template-literal",
    description: "Nested template literal — extract to a named variable.",
    remediation: "Extract the inner template to a named variable. Nested backticks are hard to read and easy to misparse.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
