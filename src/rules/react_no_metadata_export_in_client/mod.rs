//! Next.js picks up `metadata` / `generateMetadata` exports from server
//! components only. In a `"use client"` file they're ignored and the page
//! falls back to defaults, usually silently — flag the mismatch early.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
