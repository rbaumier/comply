//! react-no-effect-event-handler — `useEffect(() => { if (dep) ... }, [dep])`
//! simulates an event handler; move the logic to an actual event handler.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-effect-event-handler",
    description: "`useEffect` simulating an event handler — move logic to an actual event handler.",
    remediation: "Move the conditional logic into the event handler that sets \
                  the dependency. Effects should synchronize with external systems, \
                  not react to user events.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
