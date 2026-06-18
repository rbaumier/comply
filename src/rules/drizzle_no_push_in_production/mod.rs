//! drizzle-no-push-in-production — `drizzle-kit push` bypasses migrations.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-push-in-production",
    description: "`drizzle-kit push` is for dev only — use migrations in production/CI.",
    remediation: "Replace `drizzle-kit push` with `drizzle-kit generate` + `drizzle-kit migrate` in CI/deployment scripts.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["drizzle", "database"],

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
