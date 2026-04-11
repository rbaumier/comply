//! blank-line-between-blocks

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "blank-line-between-blocks",
    description: "Missing blank lines between logical blocks.",
    remediation: "Add a blank line before `return` statements (unless preceded by `}`) and between `const`/`let` declaration groups and function calls. Visual separation improves scannability.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
