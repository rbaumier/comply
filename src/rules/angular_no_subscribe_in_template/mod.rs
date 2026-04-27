//! angular-no-subscribe-in-template — `.subscribe()` in inline templates leaks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-subscribe-in-template",
    description: "Subscribing inside a template string fires every change-detection cycle.",
    remediation: "Use the `async` pipe in the template: `{{ data$ | async }}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
