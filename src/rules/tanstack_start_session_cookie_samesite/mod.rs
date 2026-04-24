//! tanstack-start-session-cookie-samesite — `useSession({ cookie })` must set
//! `sameSite` to `'lax'` or `'strict'`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-session-cookie-samesite",
    description: "`useSession({ cookie })` must set `sameSite` to `'lax'` or `'strict'`.",
    remediation: "Add `sameSite: 'lax'` (default) or `sameSite: 'strict'` to the \
                  cookie config to mitigate CSRF.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
