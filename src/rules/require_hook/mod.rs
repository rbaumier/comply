//! require-hook

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "require-hook",
    description: "Side effects at the top level of a test file run once at import time instead of inside a hook — tests cannot reset that state.",
    remediation: "Move side effects into beforeEach/beforeAll hooks",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jest-community/eslint-plugin-jest/blob/main/docs/rules/require-hook.md",
    ),
    categories: &["testing"],
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
