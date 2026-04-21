//! `next/headers` APIs (`cookies`, `headers`, `draftMode`) only exist on the
//! server. Importing them into a `"use client"` file throws at module
//! evaluation. Catch the misuse at authoring time.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-next-headers-in-client",
    description: "`next/headers` is server-only — importing it from a client component throws.",
    remediation: "Read headers/cookies in a server component and pass the \
                  values as props, or call a server action from the client.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/headers"),
    categories: &["react", "nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
