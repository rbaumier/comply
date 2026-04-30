//! no-gratuitous-expression

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-gratuitous-expression",
    description: "Boolean expression is always true or always false.",
    remediation: "Remove the dead branch. A condition that can never flip is either a bug or leftover from a refactor.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
