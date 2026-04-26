//! react-no-destructure-zustand-store — whole-store destructuring of a zustand hook.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-destructure-zustand-store",
    description: "Destructuring the full zustand store (`const { x, y } = useStore()`) \
                  subscribes the component to every state change.",
    remediation: "Use a selector per field: `const x = useStore(s => s.x)` so the \
                  component only re-renders when that slice changes.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
