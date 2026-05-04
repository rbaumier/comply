//! react-no-deprecated — flag usage of deprecated React / ReactDOM APIs
//! and legacy class lifecycle methods.
//!
//! Why: React has signalled the eventual removal of these APIs since
//! React 16.3. Keeping them in the codebase blocks upgrades to concurrent
//! rendering (`createRoot`, `hydrateRoot`) and hides subtle bugs in the
//! legacy lifecycle methods that fire inconsistently under Strict Mode.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-deprecated",
    description: "Deprecated React APIs should not be used.",
    remediation: "Replace the deprecated API with its modern equivalent.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jsx-eslint/eslint-plugin-react/blob/master/docs/rules/no-deprecated.md",
    ),
    categories: &["react"],
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
