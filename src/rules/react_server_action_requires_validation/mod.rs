mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-server-action-requires-validation",
    description: "Server Actions with parameters must validate input before use.",
    remediation: "Add `schema.parse(input)` or `schema.safeParse(input)` at the top of the Server Action body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
