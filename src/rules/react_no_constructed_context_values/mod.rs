//! react-no-constructed-context-values — inline object in Provider value.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-constructed-context-values",
    description: "`<Provider value={{ ... }}>` creates a new object every render, causing all consumers to re-render.",
    remediation: "Memoize the context value with `useMemo` or extract it to a \
                  stable reference. `<Provider value={memoized}>` avoids \
                  unnecessary re-renders of every consumer.",
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
