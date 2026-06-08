//! prisma-prefer-create-many-for-bulk — loop of `create()` should use `createMany`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-prefer-create-many-for-bulk",
    description: "Calling `prisma.<model>.create` inside a loop fires N round-trips — use `createMany`.",
    remediation: "Build the array of inputs first, then call `createMany({ data: inputs })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma", "performance"],

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
