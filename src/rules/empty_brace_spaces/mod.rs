//! empty-brace-spaces — disallow spaces inside empty braces.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "empty-brace-spaces",
    description: "Do not add spaces between braces.",
    remediation: "Remove whitespace between empty braces: `{  }` -> `{}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
