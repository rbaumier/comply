//! next-no-head-import-in-document

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-head-import-in-document",
    description: "Importing `next/head` in `_document` doubles head management.",
    remediation: "In `_document.tsx`, use `Head` from `next/document`. Reserve `next/head` for pages.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-head-import-in-document"),
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
