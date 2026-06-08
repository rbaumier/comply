//! next-no-document-import-in-page

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-document-import-in-page",
    description: "`next/document` is for `_document.tsx` only; importing it elsewhere breaks SSR.",
    remediation: "Move `Html`, `Main`, `NextScript` usage into `pages/_document.tsx`.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/messages/no-document-import-in-page"),
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
