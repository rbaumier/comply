//! nestjs-no-global-module-abuse — `@Global()` modules should be rare.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-global-module-abuse",
    description: "`@Global()` modules hide dependencies and break testability.",
    remediation: "Import the module explicitly where needed instead of marking it `@Global()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
