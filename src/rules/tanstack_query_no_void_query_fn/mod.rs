//! tanstack-query-no-void-query-fn — `queryFn` returning `undefined`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-void-query-fn",
    description: "`queryFn` must return data — a void / undefined return causes silent cache misses and `data: undefined` everywhere.",
    remediation: "Return the parsed response from `queryFn`. If you only need a side effect, use `useMutation` instead.",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/latest/docs/eslint/no-void-query-fn"),
    categories: &["tanstack-query"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
