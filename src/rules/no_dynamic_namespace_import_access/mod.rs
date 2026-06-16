//! no-dynamic-namespace-import-access — discourage dynamic (computed) access
//! on namespace imports, which defeats bundler tree-shaking.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-dynamic-namespace-import-access",
    description: "Accessing a namespace import dynamically (computed member access) prevents \
                  tree shaking and increases bundle size.",
    remediation: "Use a static property access (`ns.member`) or a named import instead of a \
                  computed access (`ns[expr]`).",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-dynamic-namespace-import-access/"),
    categories: &["performance", "imports"],

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
