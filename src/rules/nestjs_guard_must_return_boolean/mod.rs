//! nestjs-guard-must-return-boolean — guards must return boolean / Observable<boolean>.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-guard-must-return-boolean",
    description: "`canActivate` must return `boolean | Promise<boolean> | Observable<boolean>`.",
    remediation: "Make `canActivate` return a boolean explicitly; throw to deny instead of returning truthy/falsy.",
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
