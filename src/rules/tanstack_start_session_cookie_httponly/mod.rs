//! tanstack-start-session-cookie-httponly — `useSession({ cookie })` must set `httpOnly: true`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-session-cookie-httponly",
    description: "`useSession({ cookie })` must set `httpOnly: true`.",
    remediation: "Add `httpOnly: true` to the cookie config so session cookies \
                  cannot be read from JavaScript (XSS mitigation).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
