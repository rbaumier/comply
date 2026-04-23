mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-trusted-providers",
    description: "`accountLinking` enabled without `trustedProviders` allows any OAuth provider to link accounts.",
    remediation: "Add `trustedProviders: ['google', 'github']` to `accountLinking` to restrict which providers may link.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/account-linking"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
