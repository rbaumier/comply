mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-required-user-fields",
    description: "`user` config must declare both `email` and `name` additional fields.",
    remediation: "Add `email` and `name` to `user.additionalFields` (or your user schema).",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/concepts/database"),
    categories: &["better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
