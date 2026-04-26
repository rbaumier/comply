//! react-no-constructed-context-values — inline object in Provider value.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

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
    crate::register_ts_family!(META, react)
}
