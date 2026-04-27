//! nestjs-no-any-in-controller — `@Body() body: any` bypasses validation.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-any-in-controller",
    description: "Typing `@Body()` or `@Query()` as `any` skips the validation pipeline.",
    remediation: "Use a DTO class with class-validator decorators.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
