//! db-no-n-plus-one

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "db-no-n-plus-one",
    description: "`await db.query` inside a loop is an N+1 query — use a JOIN or batch query.",
    remediation: "Move the query outside the loop: use a JOIN, `WHERE id IN (...)`, or batch fetch. N+1 queries scale linearly with result set size and are the #1 cause of slow pages.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],

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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
