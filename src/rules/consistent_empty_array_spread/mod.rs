//! consistent-empty-array-spread

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "consistent-empty-array-spread",
    description: "Parenthesize ternaries spread into array literals.",
    remediation: "Wrap the ternary in parentheses: `[...(condition ? ['a'] : [])]` instead of `[...condition ? ['a'] : []]`. Without parens the precedence is ambiguous and confusing.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
