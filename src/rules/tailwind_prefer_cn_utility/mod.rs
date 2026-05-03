//! tailwind-prefer-cn-utility

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-cn-utility",
    description: "Ternary or concatenation in `className` should use `cn()` or `clsx()` for readability.",
    remediation: "Replace `className={x ? 'a' : 'b'}` with `className={cn('a', { b: x })}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
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
