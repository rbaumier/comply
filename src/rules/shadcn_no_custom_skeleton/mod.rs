//! shadcn-no-custom-skeleton — forbid hand-rolled skeletons built from
//! `<div className="animate-pulse …">`. Use the shadcn `<Skeleton>`
//! component instead.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-custom-skeleton",
    description: "Custom skeletons built from `animate-pulse` drift from the shadcn design tokens.",
    remediation: "Replace `<div className=\"animate-pulse …\">` with `<Skeleton className=\"…\" />`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],

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
