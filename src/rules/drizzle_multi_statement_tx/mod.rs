//! drizzle-multi-statement-tx

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-multi-statement-tx",
    description: "Sequential `db.insert`/`db.update`/`db.delete` in the same scope should run inside `db.transaction`.",
    remediation: "Wrap related mutating calls in `await db.transaction(async (tx) => { ... })` so partial failures roll back.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["drizzle"],

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
