//! array-callback-without-return

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "array-callback-without-return",
    description: "Array method callback with block body but no `return` statement.",
    remediation: "Add a `return` statement inside the callback body, or use a concise arrow expression without braces.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
