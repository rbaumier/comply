//! tanstack-start-session-secret-min-length — `useSession({ password })` must
//! reference an env var or be a literal at least 32 characters long.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-session-secret-min-length",
    description: "`useSession({ password })` must be at least 32 characters.",
    remediation: "Read the secret from an environment variable, or use a \
                  literal of at least 32 characters to prevent brute-force \
                  attacks on the session cookie.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
