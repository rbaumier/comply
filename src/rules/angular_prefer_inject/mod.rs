//! angular-prefer-inject — prefer `inject()` over constructor DI (Angular 14+).

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-prefer-inject",
    description: "Constructor injection adds boilerplate; the `inject()` function is preferred since Angular 14.",
    remediation: "Replace `constructor(private svc: Service)` with `private svc = inject(Service)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
