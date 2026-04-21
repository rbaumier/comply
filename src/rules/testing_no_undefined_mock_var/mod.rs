mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-undefined-mock-var",
    description: "`vi.mock()` factories are hoisted — module-level `let` vars they reference will be `undefined`.",
    remediation: "Declare the variable inside `vi.hoisted()` so it is initialized before the factory runs.",
    severity: Severity::Error,
    doc_url: Some("https://vitest.dev/api/vi#vi-hoisted"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
