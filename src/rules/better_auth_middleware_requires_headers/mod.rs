mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-middleware-requires-headers",
    description: "`getSession()` in middleware must forward request headers.",
    remediation: "Call `getSession({ headers: await headers() })` — otherwise session lookup fails in middleware context.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/integrations/next#middleware"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
