//! unicorn-prefer-array-flat-map — `.map(...).flat()` → `.flatMap(...)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "unicorn-prefer-array-flat-map",
    description: "`.map(fn).flat()` walks the array twice — `.flatMap(fn)` does it once.",
    remediation: "Replace `xs.map(fn).flat()` with `xs.flatMap(fn)`. Same shape, half the work.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/sindresorhus/eslint-plugin-unicorn/blob/main/docs/rules/prefer-array-flat-map.md"),
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
