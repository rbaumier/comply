mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-require-rate-limit",
    description: "Better Auth config without `rateLimit` leaves auth endpoints unprotected.",
    remediation: "Add `rateLimit: { enabled: true }` to your `betterAuth({})` config.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/rate-limiting"),
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
