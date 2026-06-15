//! no-react-specific-props — flag React-specific JSX props in non-React JSX.
//!
//! `className` and `htmlFor` are React-only prop names. In non-React JSX
//! frameworks (Solid, Qwik, Vue JSX, Preact, Stencil) the DOM-native forms
//! `class` and `for` are the correct attributes, and the React names are not
//! supported. The rule fires only in non-React JSX files (so it stays silent
//! in React projects, where `className`/`htmlFor` are correct) and suggests
//! the native replacement.
//!
//! Ported from Biome's `noReactSpecificProps`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-react-specific-props",
    description: "React-specific JSX prop used in a non-React framework.",
    remediation: "Replace the React-specific prop with its DOM-native form \
                  (`className` → `class`, `htmlFor` → `for`).",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-react-specific-props/"),
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
