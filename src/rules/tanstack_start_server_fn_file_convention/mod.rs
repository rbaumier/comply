mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-file-convention",
    description: "`createServerFn` must live in a `.functions.ts` file to enforce server/client separation.",
    remediation: "Move `createServerFn` calls to a file named `*.functions.ts`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
