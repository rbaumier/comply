//! `generateStaticParams` runs at build time on the server to pre-render
//! dynamic routes. Exporting it from a `"use client"` file is a no-op — the
//! build skips it and all paths are rendered dynamically. Flag early.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-generate-static-params-in-client",
    description: "Next.js ignores `generateStaticParams` exports from client components.",
    remediation: "Move `generateStaticParams` to a server `page.tsx` (no \
                  `\"use client\"`) and import the client component as a child.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/generate-static-params"),
    categories: &["react", "nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
