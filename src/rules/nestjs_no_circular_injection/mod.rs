//! nestjs-no-circular-injection — `forwardRef()` indicates circular DI.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-circular-injection",
    description: "`forwardRef()` reveals a circular dependency between providers.",
    remediation: "Refactor to break the cycle — extract shared logic into a third service.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
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
