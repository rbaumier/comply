//! next-no-server-import-in-client

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-server-import-in-client",
    description: "Server-only modules (`fs`, `net`, `next/server`, `server-only`) cannot run in the browser.",
    remediation: "Move the import to a server module, or remove the `\"use client\"` directive.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/rendering/composition-patterns"),
    categories: &["nextjs", "rsc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
