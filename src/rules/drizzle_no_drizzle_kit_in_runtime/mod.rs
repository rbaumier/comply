//! drizzle-no-drizzle-kit-in-runtime

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-drizzle-kit-in-runtime",
    description: "`drizzle-kit` is a CLI/dev-time package — importing it from runtime code pulls migration tooling into the production bundle.",
    remediation: "Keep `drizzle-kit` imports inside `drizzle.config.ts` or migration scripts; runtime code should depend only on `drizzle-orm`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle", "bundle"],

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
