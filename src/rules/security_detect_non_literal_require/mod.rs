//! security-detect-non-literal-require — `require(<non-literal>)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-detect-non-literal-require",
    description: "`require(<dynamic>)` loads a module path computed at runtime — a known supply-chain / RCE vector when the input can be influenced by user data.",
    remediation: "Use a static `require(\"module-name\")` literal. If a dynamic import is genuinely needed, validate the path against a whitelist before passing it to `require` / `import()`.",
    severity: Severity::Error,
    doc_url: Some("https://github.com/eslint-community/eslint-plugin-security/blob/main/docs/rules/detect-non-literal-require.md"),
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
