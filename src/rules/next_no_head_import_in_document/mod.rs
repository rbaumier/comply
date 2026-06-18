//! next-no-head-import-in-document

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-head-import-in-document",
    description: "Importing `next/head` in `_document` doubles head management.",
    remediation: "In `_document.tsx`, use `Head` from `next/document`. Reserve `next/head` for pages.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-head-import-in-document"),
    categories: &["nextjs"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
