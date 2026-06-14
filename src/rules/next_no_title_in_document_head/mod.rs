//! next-no-title-in-document-head — `<title>` inside `_document.tsx` `<Head>`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-title-in-document-head",
    description: "Putting `<title>` inside `_document.tsx`'s `<Head>` makes every page share the same title.",
    remediation: "Move `<title>` to per-page `<Head>` from `next/head`, or use the App Router's `metadata` export.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-title-in-document-head"),
    categories: &["nextjs"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check)))],
    }
}
