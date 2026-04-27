//! next-no-client-import-in-server

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-client-import-in-server",
    description: "Browser-only modules cannot be imported into server components.",
    remediation: "Move the import into a `\"use client\"` boundary, or replace it with a server-safe alternative.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/rendering/composition-patterns"),
    categories: &["nextjs", "rsc"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
