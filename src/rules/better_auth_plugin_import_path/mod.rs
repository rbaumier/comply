mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-plugin-import-path",
    description: "Importing from `better-auth/plugins` barrel prevents tree-shaking.",
    remediation: "Import from the plugin's specific path: `better-auth/plugins/two-factor`.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/plugins"),
    categories: &["better-auth", "imports"],
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
