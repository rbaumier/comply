//! jsx-no-new-function-as-prop — disallow newly created functions as JSX prop values.
//!
//! An arrow function or function expression written directly inside a JSX prop
//! (`onClick={() => ...}`, `onChange={function () {}}`) allocates a brand-new
//! function on every render. That fresh reference breaks `React.memo` /
//! `PureComponent` equality checks and forces the child to re-render even when
//! nothing meaningful changed.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsx-no-new-function-as-prop",
    description: "Arrow/function expressions as JSX prop values create a new reference every render.",
    remediation: "Hoist the handler with `useCallback` or to a stable identifier defined outside the render.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/NickvanDyke/eslint-plugin-react-perf#rules"),
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
