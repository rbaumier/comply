//! zod-prefer-extend-over-merge — prefer `.extend(...)` over `.merge(...)` (Zod v4).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-prefer-extend-over-merge",
    description: "`.merge()` is removed in Zod v4 — `.extend()` is the canonical \
                  way to augment an object schema.",
    remediation: "Replace `A.merge(B)` with `A.extend(B.shape)` (or inline the fields).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
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
