//! sonarjs-no-useless-catch — `catch (e) { throw e; }`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sonarjs-no-useless-catch",
    description: "`catch (e) { throw e; }` adds no value — remove the try/catch.",
    remediation: "Delete the try/catch and let the exception propagate. If you wanted to add context, wrap the error or convert to a typed Result; if you wanted to log, log alongside the rethrow.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/SonarSource/eslint-plugin-sonarjs/blob/master/docs/rules/no-useless-catch.md"),
    categories: &["code-quality"],
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
