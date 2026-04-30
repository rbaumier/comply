//! tanstack-start-no-client-import-in-server-fn

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-no-client-import-in-server-fn",
    description: "Client-only React imports in a `.functions.ts` file — server functions cannot use browser APIs.",
    remediation: "Move client-only logic out of `.functions.ts`. Only import server-safe deps.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["tanstack", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
