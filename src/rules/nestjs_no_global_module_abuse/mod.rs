//! nestjs-no-global-module-abuse — `@Global()` modules should be rare.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-global-module-abuse",
    description: "`@Global()` modules hide dependencies and break testability.",
    remediation: "Import the module explicitly where needed instead of marking it `@Global()`.",
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
