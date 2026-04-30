mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "express-session-require-name",
    description: "`session({...})` config is missing the `name` property — the default session cookie name is predictable.",
    remediation: "Add name property to session config to avoid default session name",
    severity: Severity::Warning,
    doc_url: Some("https://expressjs.com/en/resources/middleware/session.html#options"),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
