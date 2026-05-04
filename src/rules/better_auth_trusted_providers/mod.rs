mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-trusted-providers",
    description: "`accountLinking` enabled without `trustedProviders` allows any OAuth provider to link accounts.",
    remediation: "Add `trustedProviders: ['google', 'github']` to `accountLinking` to restrict which providers may link.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/account-linking"),
    categories: &["security", "better-auth"],
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
