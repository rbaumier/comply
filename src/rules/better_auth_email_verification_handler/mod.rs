mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-email-verification-handler",
    description: "`emailVerification.sendOnSignUp: true` requires `sendVerificationEmail`.",
    remediation: "Define `sendVerificationEmail(user, url)` in the `emailVerification` block.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/authentication/email-password"),
    categories: &["better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
