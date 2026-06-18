//! next-no-img-element

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-img-element",
    description: "Using `<img>` instead of `next/image` disables image optimization.",
    remediation: "Replace `<img>` with `<Image>` from `next/image` to enable lazy loading and automatic resizing.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/messages/no-img-element"),
    categories: &["nextjs", "performance"],

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
