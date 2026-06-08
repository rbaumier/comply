//! `next/headers` APIs (`cookies`, `headers`, `draftMode`) only exist on the
//! server. Importing them into a `"use client"` file throws at module
//! evaluation. Catch the misuse at authoring time.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-next-headers-in-client",
    description: "`next/headers` is server-only — importing it from a client component throws.",
    remediation: "Read headers/cookies in a server component and pass the \
                  values as props, or call a server action from the client.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/functions/headers"),
    categories: &["react", "nextjs"],

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
