//! react-no-find-in-map-loop — `.find()`/`.filter()` nested inside `.map()` or a `for` loop.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-find-in-map-loop",
    description: "`.find()` / `.filter()` called inside a `.map()` callback or `for` loop \
                  turns an O(n) pass into O(n²).",
    remediation: "Build a `Map`/lookup index once, then look up inside the loop.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "code-quality"],

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
