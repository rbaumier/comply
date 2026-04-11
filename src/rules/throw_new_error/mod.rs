//! throw-new-error — require `new` when creating an error.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "throw-new-error",
    description: "Use `new` when creating an error.",
    remediation: "Replace `throw Error(...)` with `throw new Error(...)`. \
                  Calling Error without `new` is valid but inconsistent and \
                  can confuse readers about whether a new instance is created.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
