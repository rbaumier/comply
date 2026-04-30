//! react-no-this-in-sfc — `this.` inside a functional component.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-this-in-sfc",
    description: "`this` has no meaning inside a functional component.",
    remediation: "Remove `this.` references. Functional components don't have a \
                  `this` context — use hooks (`useState`, `useRef`, etc.) instead \
                  of `this.state`, `this.props`, etc.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
