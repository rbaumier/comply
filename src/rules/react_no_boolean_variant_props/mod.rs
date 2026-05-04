//! react-no-boolean-variant-props — 2+ `isX`/`hasX` boolean props on a component.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-boolean-variant-props",
    description: "A component declaring two or more `isX` / `hasX` boolean props is \
                  almost always modeling mutually-exclusive variants — 2^N invalid states \
                  become representable.",
    remediation: "Replace the booleans with a single `variant: 'primary' | 'ghost' | ...` \
                  prop.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
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
