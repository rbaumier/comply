//! expression-complexity

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "expression-complexity",
    description: "Overly complex expression with too many logical/conditional operators.",
    remediation: "Extract parts of the expression into named intermediate variables. Lines with 4+ logical/conditional operators are hard to read and reason about.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
