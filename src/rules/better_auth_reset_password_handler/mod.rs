mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-reset-password-handler",
    description: "`emailAndPassword.enabled: true` requires a `sendResetPassword` handler.",
    remediation: "Define `sendResetPassword({ user, url })` in the `emailAndPassword` block.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/authentication/email-password"),
    categories: &["better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
