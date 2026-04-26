//! Next.js picks up `metadata` / `generateMetadata` exports from server
//! components only. In a `"use client"` file they're ignored and the page
//! falls back to defaults, usually silently — flag the mismatch early.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-metadata-export-in-client",
    description: "Next.js ignores `metadata` exports from client components.",
    remediation: "Move the metadata export to a separate server component \
                  (e.g. `layout.tsx` or `page.tsx` without `\"use client\"`), \
                  and import your client component from there.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/building-your-application/optimizing/metadata"),
    categories: &["react", "nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
