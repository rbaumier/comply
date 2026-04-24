//! react-no-usestate-high-frequency — `setState` inside high-frequency event handlers.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-usestate-high-frequency",
    description: "`setState` inside `mousemove`/`scroll`/`resize`/`pointermove` handlers \
                  schedules a render on every frame (or faster).",
    remediation: "Store the transient value in a `useRef` and read it when you actually \
                  need to commit a render (e.g. on drag end).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
