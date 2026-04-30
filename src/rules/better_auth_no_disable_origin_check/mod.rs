mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-disable-origin-check",
    description: "`disableOriginCheck: true` removes origin validation from Better Auth.",
    remediation: "Remove `disableOriginCheck` — origin validation prevents cross-origin request forgery.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/security"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
