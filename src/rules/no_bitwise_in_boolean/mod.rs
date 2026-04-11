//! no-bitwise-in-boolean

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-bitwise-in-boolean",
    description: "Bitwise operators in boolean contexts are likely typos.",
    remediation: "Use `&&` instead of `&`, `||` instead of `|`. Bitwise operators in `if`/`while` conditions are almost always a mistake.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
