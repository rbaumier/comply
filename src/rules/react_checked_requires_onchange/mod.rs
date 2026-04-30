//! react-checked-requires-onchange — checked without onChange or readOnly.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-checked-requires-onchange",
    description: "`checked` prop without `onChange` or `readOnly` makes the input uncontrollable.",
    remediation: "Add an `onChange` handler or `readOnly` prop. Without either, \
                  React renders a frozen checkbox/radio that the user cannot \
                  interact with, and emits a console warning.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    let backends = crate::register_ts_family!(META, react).backends;
    RuleDef {
        meta: META,
        backends,
    }
}
