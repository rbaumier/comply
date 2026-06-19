//! drizzle-no-redundant-roundtrip-around-write

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-redundant-roundtrip-around-write",
    description: "A Drizzle existence-probe guarding a write on the same table+key, or a re-read of a just-written row, is a redundant non-atomic round-trip.",
    remediation: "Fold the probe into the write with `.onConflictDoNothing()` / `.onConflictDoUpdate()`, and read the written row back with `.returning()` instead of a follow-up query.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/insert#upserts-and-conflicts"),
    categories: &["drizzle", "database", "performance"],

    skip_in_test_dir: true,
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
