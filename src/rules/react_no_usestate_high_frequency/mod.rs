//! react-no-usestate-high-frequency — `setState` inside high-frequency event handlers.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
