//! shadcn-no-custom-badge — forbid badge-looking `<span>` built from
//! `rounded-full bg-*` utilities. Use the shadcn `<Badge>` component
//! so variants stay consistent.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "shadcn-no-custom-badge",
    description: "Badge-shaped `<span>` drifts from the shadcn design system — use `<Badge>`.",
    remediation: "Replace `<span className=\"rounded-full bg-…\">` with `<Badge variant=\"…\">`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["shadcn"],
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
