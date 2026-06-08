//! react-no-html-entities-in-jsx — useless HTML entities in JSX.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-html-entities-in-jsx",
    description: "HTML entities like `&apos;`, `&quot;`, `&amp;`, `&gt;` are noise in JSX — React encodes raw characters automatically.",
    remediation: "Replace the entity with the raw character: `&apos;` -> `'`, `&quot;` -> `\"`, `&amp;` -> `&`, `&gt;` -> `>`. \
                  `&lt;` (for a literal `<`) and `&nbsp;` (non-breaking space) are kept as legitimate.",
    severity: Severity::Warning,
    doc_url: None,
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
