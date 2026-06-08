//! no-uniq-key

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-uniq-key",
    description: "Non-unique key in JSX list — `Math.random()`, `Date.now()`, or `uuid()` create new keys every render.",
    remediation: "Use a stable, unique identifier from the data (e.g., `item.id`). Random keys destroy React's reconciliation and cause performance issues.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],

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
