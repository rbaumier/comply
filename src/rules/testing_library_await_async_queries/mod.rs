//! testing-library-await-async-queries — `findBy*` must be awaited.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-library-await-async-queries",
    description: "`findBy*` / `findAllBy*` queries return a Promise — used without `await` they resolve to an unwrapped Promise object.",
    remediation: "`await screen.findByText(\"x\")` (or `.then(...)`). For sync lookups use `getBy*` / `queryBy*` instead.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/testing-library/eslint-plugin-testing-library/blob/main/docs/rules/await-async-queries.md"),
    categories: &["testing", "testing-library"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
