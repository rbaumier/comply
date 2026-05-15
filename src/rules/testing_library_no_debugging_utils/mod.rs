//! testing-library-no-debugging-utils — `screen.debug()` left in tests.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "testing-library-no-debugging-utils",
    description: "Debug helpers (`screen.debug()`, `prettyDOM()`, `logRoles()`) left in committed tests pollute CI output.",
    remediation: "Delete the debug call before committing, or wrap it in `if (process.env.DEBUG)` if it's a temporary affordance.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/testing-library/eslint-plugin-testing-library/blob/main/docs/rules/no-debugging-utils.md"),
    categories: &["testing", "testing-library"],
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
