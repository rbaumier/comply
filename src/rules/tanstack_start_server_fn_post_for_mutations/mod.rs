//! tanstack-start-server-fn-post-for-mutations — mutation-named server
//! functions must use `method: 'POST'`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-post-for-mutations",
    description: "Mutation-named server functions must use `method: 'POST'`.",
    remediation: "Pass `{ method: 'POST' }` to `createServerFn` when the fn \
                  performs create/update/delete/login/logout side effects.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
