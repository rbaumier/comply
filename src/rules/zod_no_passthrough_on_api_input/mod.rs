//! zod-no-passthrough-on-api-input — `.passthrough()` lets unknown
//! keys through. On API input schemas this is a footgun: clients can
//! smuggle fields that downstream code may persist.

#[cfg(test)]
mod typescript;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-passthrough-on-api-input",
    description: "`.passthrough()` on API input schemas lets unknown keys through.",
    remediation: "Use `.strict()` to reject unknown keys, or remove `.passthrough()` and let zod strip them.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
