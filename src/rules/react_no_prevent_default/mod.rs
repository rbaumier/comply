//! react-no-prevent-default — `event.preventDefault()` inside passive event
//! listeners (`onScroll`, `onWheel`, `onTouchStart`, `onTouchMove`) is a no-op
//! because React attaches these listeners as passive by default.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-prevent-default",
    description: "`event.preventDefault()` inside passive event listeners (`onScroll`, \
                  `onWheel`, `onTouchStart`, `onTouchMove`) is a no-op.",
    remediation: "Remove the `preventDefault()` call. If you actually need to cancel the event, \
                  attach the listener manually via `addEventListener(name, handler, { passive: false })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
