//! no-async-array-callback — flag `arr.forEach(async ...)` and friends.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-async-array-callback",
    description: "`async` callback passed to a non-awaiting array method.",
    remediation: "`forEach`/`map`/`filter`/`some`/`every`/`find` don't await their \
                  callbacks — your async work runs in parallel (or not at all) and \
                  rejections become unhandled. Use `for (const x of arr)` with \
                  `await` inside, or `Promise.all(arr.map(async ...))` when you \
                  want parallel + awaited.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["async"],

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
