//! react-no-state-setter-in-render — calling a `useState` setter directly
//! in the component body causes an infinite render loop (or, with the
//! conditional updater pattern, React's "Maximum update depth exceeded").

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-state-setter-in-render",
    description: "`setState(...)` called directly during render — triggers an infinite render loop.",
    remediation: "Move the setter into an event handler or `useEffect`. If you need to derive state, \
                  compute it during render instead of storing it.",
    severity: Severity::Error,
    doc_url: Some("https://react.dev/learn/you-might-not-need-an-effect"),
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
