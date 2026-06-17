//! api-separate-input-output-types — flag *exported* type/interface
//! declarations that mix server-managed fields (`id`, `createdAt`,
//! `updatedAt`) with fields meant for client input. Such shapes tend to
//! get reused as both request input and response output, leaking
//! implementation details into the write path and forcing clients to send
//! fields they shouldn't own. Non-exported, file-local helper types never
//! reach an API boundary, so they are not analyzed.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "api-separate-input-output-types",
    description: "The same type must not serve as both request input and response output.",
    remediation: "Split into separate input (CreateXInput) and output (XResponse) types. Server-managed fields (id, createdAt, updatedAt) belong only in the output shape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],

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
