//! angular-no-direct-dom — direct DOM access bypasses Angular's rendering pipeline.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-direct-dom",
    description: "Direct DOM access bypasses Angular's rendering pipeline.",
    remediation: "Use `Renderer2` or `@ViewChild` instead of `document.getElementById`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
