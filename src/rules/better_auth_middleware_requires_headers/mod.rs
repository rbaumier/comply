mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-middleware-requires-headers",
    description: "`getSession()` in middleware must forward request headers.",
    remediation: "Call `getSession({ headers: await headers() })` — otherwise session lookup fails in middleware context.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/integrations/next#middleware"),
    categories: &["security", "auth"],
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
