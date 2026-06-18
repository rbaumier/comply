//! prisma-no-delete-without-where — `deleteMany()` without where wipes the table.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-delete-without-where",
    description: "`deleteMany()` without `where` deletes every row in the table.",
    remediation: "Add `where: { ... }`. If you really mean to wipe the table, do it from a maintenance script.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["prisma", "safety"],

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
