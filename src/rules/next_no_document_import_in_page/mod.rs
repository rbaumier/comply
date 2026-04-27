//! next-no-document-import-in-page

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-document-import-in-page",
    description: "`next/document` is for `_document.tsx` only; importing it elsewhere breaks SSR.",
    remediation: "Move `Html`, `Main`, `NextScript` usage into `pages/_document.tsx`.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-document-import-in-page"),
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
