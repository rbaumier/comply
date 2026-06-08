//! prisma-select-only-needed-fields — `findMany` without `select`/`include`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-select-only-needed-fields",
    description: "`findMany` without `select` fetches every column — wasteful for wide tables.",
    remediation: "Add `select: { id: true, ... }` (or `include` for relations) to fetch only what's needed.",
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
