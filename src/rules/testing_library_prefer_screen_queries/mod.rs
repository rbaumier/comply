//! testing-library-prefer-screen-queries — use `screen.getBy*` over `render()` destructure.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-library-prefer-screen-queries",
    description: "Destructuring `getBy*` / `findBy*` from `render(...)` is the legacy form — prefer `screen.getBy*` for stable refactoring.",
    remediation: "Stop destructuring from `render(...)` — call `render(<UI/>)` for its side effect and use `screen.getBy*` / `screen.findBy*` everywhere.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/testing-library/eslint-plugin-testing-library/blob/main/docs/rules/prefer-screen-queries.md"),
    categories: &["testing", "testing-library"],
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
