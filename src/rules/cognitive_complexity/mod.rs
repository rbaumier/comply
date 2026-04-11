//! cognitive-complexity

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "cognitive-complexity",
    description: "Function cognitive complexity exceeds 5.",
    remediation: "Simplify by extracting helpers, removing nesting, or splitting into smaller functions. Cognitive complexity measures how hard a function is to understand.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
