mod oxc_typescript;
#[cfg(test)]
mod typescript;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-prefer-using-declaration",
    description: "try/finally with a single cleanup call is replaceable by `using` / `await using` (TS 5.2+).",
    remediation: "Declare the resource with `using res = ...` and let the runtime call dispose.",
    severity: Severity::Warning,
    doc_url: Some("https://www.typescriptlang.org/docs/handbook/release-notes/typescript-5-2.html"),
    categories: &["typescript"],
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
