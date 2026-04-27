//! nestjs-guard-must-return-boolean — guards must return boolean / Observable<boolean>.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-guard-must-return-boolean",
    description: "`canActivate` must return `boolean | Promise<boolean> | Observable<boolean>`.",
    remediation: "Make `canActivate` return a boolean explicitly; throw to deny instead of returning truthy/falsy.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
