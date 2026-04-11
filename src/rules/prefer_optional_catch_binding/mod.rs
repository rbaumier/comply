//! prefer-optional-catch-binding — prefer omitting unused `catch` binding parameter.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-optional-catch-binding",
    description: "Prefer omitting the `catch` binding parameter when it is unused.",
    remediation: "Remove the unused catch binding: use `catch { … }` instead of \
                  `catch (error) { … }`. Optional catch binding is supported in ES2019+.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
