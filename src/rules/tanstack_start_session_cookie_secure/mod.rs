//! tanstack-start-session-cookie-secure — `useSession({ cookie })` must set `secure`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-session-cookie-secure",
    description: "`useSession({ cookie })` must set `secure`.",
    remediation: "Add `secure: true` (or `secure: process.env.NODE_ENV === 'production'`) \
                  so the cookie is only sent over HTTPS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
