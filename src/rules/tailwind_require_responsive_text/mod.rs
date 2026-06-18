//! tailwind-require-responsive-text — large heading text (`text-4xl`+)
//! without a responsive variant overflows on phones. Require at least one
//! `sm:text-*` / `md:text-*` / `lg:text-*` when the base size is big.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-require-responsive-text",
    description: "Headings with `text-4xl+` must also declare a responsive size variant.",
    remediation: "Scale the heading down on mobile, e.g. `text-2xl md:text-4xl` instead of just `text-4xl`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],

    skip_in_test_dir: true,
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
