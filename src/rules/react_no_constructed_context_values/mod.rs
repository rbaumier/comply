//! react-no-constructed-context-values — inline object in Provider value.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

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
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
