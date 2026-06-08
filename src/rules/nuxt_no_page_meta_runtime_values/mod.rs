//! nuxt-no-page-meta-runtime-values — runtime expressions in definePageMeta.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nuxt-no-page-meta-runtime-values",
    description: "`definePageMeta({...})` is statically analysed at build time — runtime expressions, variable references, or function calls in its properties are dropped.",
    remediation: "Use only literals (strings, numbers, booleans, arrays of literals, object literals) in `definePageMeta`. Move dynamic values to a `setup()` block or middleware.",
    severity: Severity::Error,
    doc_url: Some("https://nuxt.com/docs/api/utils/define-page-meta"),
    categories: &["nuxt"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
