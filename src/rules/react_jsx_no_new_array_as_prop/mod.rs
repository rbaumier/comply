//! react-jsx-no-new-array-as-prop — disallow array literals as JSX prop values.
//!
//! An array literal written directly inside a JSX prop (`items={[1, 2, 3]}`)
//! allocates a new array on every render. That new reference breaks
//! `React.memo` / `PureComponent` equality checks and forces the child
//! component to re-render even when the contents are identical.
//!
//! Skipped in test files and Storybook stories, where a component renders once
//! and reference identity is irrelevant. Also skipped when the project ships
//! `babel-plugin-react-compiler` (declared in `package.json` or referenced from
//! a bundler / babel config), since React Compiler auto-memoises inline
//! references.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-new-array-as-prop",
    description: "Array literals as JSX prop values create a new reference every render.",
    remediation: "Extract array to a constant or use useMemo",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    RuleDef {
        meta: META,
        backends,
    }
}
