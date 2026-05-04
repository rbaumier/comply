mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-reset-password-handler",
    description: "`emailAndPassword.enabled: true` requires a `sendResetPassword` handler.",
    remediation: "Define `sendResetPassword({ user, url })` in the `emailAndPassword` block.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/authentication/email-password"),
    categories: &["better-auth"],
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
