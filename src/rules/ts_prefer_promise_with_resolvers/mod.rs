//! ts-prefer-promise-with-resolvers — flag the `new Promise(...)` constructor
//! and steer users toward the modern `Promise.withResolvers()` API which
//! avoids the executor-callback closure pattern.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-promise-with-resolvers",
    description: "`new Promise(...)` is verbose — prefer `Promise.withResolvers()` to obtain `{ promise, resolve, reject }` without an executor closure.",
    remediation: "Replace `new Promise((resolve, reject) => { ... })` with \
                  `const { promise, resolve, reject } = Promise.withResolvers();` \
                  and call `resolve`/`reject` from wherever you previously did.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise/withResolvers",
    ),
    categories: &["typescript", "async"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
