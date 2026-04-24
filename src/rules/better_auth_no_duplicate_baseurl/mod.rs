mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-duplicate-baseurl",
    description: "Avoid hardcoding `baseURL` in `betterAuth()` — rely on `BETTER_AUTH_URL`.",
    remediation:
        "Remove `baseURL` from the config and set `BETTER_AUTH_URL` in the environment instead.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/installation"),
    categories: &["better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
