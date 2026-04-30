mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-requires-auth",
    description: "`createServerFn` handlers with DB mutations must verify authentication.",
    remediation: "Call `getSession()` or `auth()` at the top of the handler and throw if no session.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
