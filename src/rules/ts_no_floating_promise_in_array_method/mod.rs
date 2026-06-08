//! ts-no-floating-promise-in-array-method — `.forEach(async ...)` and
//! `.map(async ...)` create promises that are never awaited; rejections
//! become `UnhandledPromiseRejection` and the caller can't observe completion.

#[cfg(test)] mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-floating-promise-in-array-method",
    description: "`.forEach(async ...)` / `.map(async ...)` floats promises — the iteration completes before the async work does.",
    remediation: "Use `for (const x of arr) { await fn(x); }` for sequential work, \
                  or `await Promise.all(arr.map(async (x) => fn(x)))` for parallel work.",
    severity: Severity::Warning,
    doc_url: None,
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
