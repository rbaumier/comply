//! react-no-object-type-as-default-prop — mutable default prop breaks memo.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-object-type-as-default-prop",
    description: "Object/array/function default props create a new reference every render, breaking `React.memo`.",
    remediation: "Move default values to a module-level constant or use `useMemo`/`useCallback`. \
                  `function Foo({ items = DEFAULT_ITEMS })` with `const DEFAULT_ITEMS = []` \
                  outside the component keeps a stable reference.",
    severity: Severity::Warning,
    doc_url: None,
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
