//! react-no-prevent-default — `event.preventDefault()` inside passive event
//! listeners (`onScroll`, `onWheel`, `onTouchStart`, `onTouchMove`) is a no-op
//! because React attaches these listeners as passive by default.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
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
