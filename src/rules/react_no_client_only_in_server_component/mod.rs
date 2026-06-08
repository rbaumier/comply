//! `client-only` is the mirror of `server-only`: importing it from a server
//! component throws at module evaluation. Flag the mismatch early.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-client-only-in-server-component",
    description: "`client-only` can't be imported from a server component.",
    remediation: "Mark the file `\"use client\"`, or remove the `client-only` \
                  import and keep the module server-safe.",
    severity: Severity::Error,
    doc_url: Some("https://www.npmjs.com/package/client-only"),
    categories: &["react"],

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
