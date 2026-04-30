//! elseif-without-else

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "elseif-without-else",
    description: "`if/else if` chain without a final `else` clause.",
    remediation: "Add a final `else` block to handle all remaining cases explicitly, even if it's just a comment or unreachable assertion.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
