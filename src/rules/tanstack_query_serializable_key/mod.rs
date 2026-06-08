mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-serializable-key",
    description: "Query keys must be structurally serializable — no functions, Dates, Symbols, or class instances.",
    remediation: "Serialize the value first: use `date.toISOString()` instead of `new Date()`, a string tag instead of a Symbol, and a plain identifier instead of a closure or class instance.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/query-keys"),
    categories: &["tanstack-query"],

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
