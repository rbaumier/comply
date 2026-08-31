//! Spreading before `.map()` copies an array that `.map()` reallocates anyway.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-array-from-map",
    description: "`[...x].map(fn)` builds an intermediate array before mapping.",
    remediation: "Drop the spread when `x` is already an array — `x.map(fn)` returns a new array on its own. When `x` is a `Map`, a `Set` or another iterable, use `Array.from(x, fn)` to map without materializing the intermediate array.",
    severity: Severity::Error,
    doc_url: Some(
        "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/from",
    ),
    categories: &["unicorn", "performance"],

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
