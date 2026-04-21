mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-prefer-test-each",
    description: "3+ tests with a common name prefix can be collapsed into a single `test.each` table.",
    remediation: "Use `test.each([...])` to express parameterized cases without repeating test boilerplate.",
    severity: Severity::Warning,
    doc_url: None,
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
