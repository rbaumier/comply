//! catch-error-name — enforce `catch (error)` naming convention.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "catch-error-name",
    description: "The catch parameter should be named `error`.",
    remediation: "Rename the catch parameter to `error` (or `error_` if shadowed). \
                  Using `_` is allowed when the parameter is unused.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
