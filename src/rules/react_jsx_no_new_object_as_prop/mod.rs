//! react-jsx-no-new-object-as-prop — disallow inline object literals as JSX prop values.
//!
//! An object literal written directly inside a JSX prop (`style={{ color: 'red' }}`,
//! `config={{ a: 1 }}`) allocates a new object on every render. That new reference
//! breaks `React.memo` / `PureComponent` equality checks and forces memoized children
//! to re-render even when the logical value is unchanged.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-jsx-no-new-object-as-prop",
    description: "Object literals passed directly as JSX props create a new reference every render.",
    remediation: "Extract object to a constant or use useMemo",
    severity: Severity::Warning,
    doc_url: None,
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
