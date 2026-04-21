mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-disable-origin-check",
    description: "`disableOriginCheck: true` removes origin validation from Better Auth.",
    remediation: "Remove `disableOriginCheck` — origin validation prevents cross-origin request forgery.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/security"),
    categories: &["security", "better-auth"],
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
