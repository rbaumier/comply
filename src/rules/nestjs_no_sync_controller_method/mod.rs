//! nestjs-no-sync-controller-method — controller methods should be async.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-sync-controller-method",
    description: "Controller route handlers should be `async` so NestJS can await them uniformly.",
    remediation: "Add `async` to the method or have it return a `Promise`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
