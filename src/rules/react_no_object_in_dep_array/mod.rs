//! react-no-object-in-dep-array — flag non-primitive values in hook
//! dependency arrays.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-object-in-dep-array",
    description:
        "Hook dep arrays must not contain values that allocate every render.",
    remediation: "Move inline object/array literals, inline functions, and \
                  `new Map()`-style allocations out of the dep array. Extract \
                  them into `useMemo`/`useCallback`, or depend on primitive \
                  fields that are stable across renders.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
