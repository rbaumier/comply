//! nestjs-dto-needs-validation — `*Dto` classes should use class-validator.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-dto-needs-validation",
    description: "DTO classes should decorate their fields with `class-validator` constraints.",
    remediation: "Add `@IsString()`, `@IsNumber()`, `@IsEmail()`, etc. to each property.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
