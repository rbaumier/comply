//! tanstack-start-server-fn-use-notfound — prefer `throw notFound()` over
//! `throw new Error('not found')` in server functions.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-use-notfound",
    description: "Server functions should throw `notFound()` rather than a generic Error.",
    remediation: "Replace `throw new Error('not found')` with `throw notFound()` \
                  so the router renders the proper 404 boundary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],

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
