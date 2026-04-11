//! no-logger-in-business-logic

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-logger-in-business-logic",
    description: "Logging calls in business logic (service/domain/core/model/entity layers).",
    remediation: "Remove direct `logger.*` / `console.log` calls from business logic. Use a `withLogging()` wrapper or emit domain events instead. Logging is a cross-cutting concern — it belongs in infrastructure, not domain code.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
