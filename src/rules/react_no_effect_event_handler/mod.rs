//! react-no-effect-event-handler — `useEffect(() => { if (dep) ... }, [dep])`
//! simulates an event handler; move the logic to an actual event handler.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
