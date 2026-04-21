mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-and-in-test-name",
    description: "Test names containing \" and \" usually test multiple behaviors — split into separate tests.",
    remediation: "Write one test per behavior; use `describe` to group related cases.",
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
