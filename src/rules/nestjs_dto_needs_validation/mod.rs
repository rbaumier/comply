//! nestjs-dto-needs-validation — `*Dto` classes should use class-validator.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-dto-needs-validation",
    description: "DTO classes should decorate their fields with `class-validator` constraints.",
    remediation: "Add `@IsString()`, `@IsNumber()`, `@IsEmail()`, etc. to each property.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
