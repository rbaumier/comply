//! prefer-single-boolean-return

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-single-boolean-return",
    description: "`if (cond) return true; else return false;` can be replaced by `return cond;`.",
    remediation: "Return the condition (or its negation) directly: `return cond;` or `return !cond;`.",
    severity: Severity::Warning,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S1126"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
