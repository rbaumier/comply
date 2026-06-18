//! ts-prefer-promise-with-resolvers — steer users toward `Promise.withResolvers()`
//! when a `new Promise(executor)` leaks its `resolve`/`reject` handle out of the
//! executor's scope (assigns it to an outer binding, stores it on an object, or
//! returns it). Self-contained executors are left alone.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-promise-with-resolvers",
    description: "An executor that leaks `resolve`/`reject` out of its scope is better written with `Promise.withResolvers()`, which returns `{ promise, resolve, reject }` directly.",
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
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
