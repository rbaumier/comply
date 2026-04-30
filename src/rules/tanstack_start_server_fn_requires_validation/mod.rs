mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-requires-validation",
    description: "`createServerFn` handlers must validate their input with `.input()` or `.safeParse()`.",
    remediation: "Chain `.input(z.object({...}))` before `.handler(...)` to validate at the RPC boundary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
