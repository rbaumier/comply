//! no-fire-event — prefer `userEvent` over `fireEvent` in tests.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-fire-event",
    description: "`fireEvent` dispatches a single synthetic event — `userEvent` reproduces the full browser event sequence.",
    remediation: "Replace `fireEvent.click()` / `fireEvent.change()` with \
                  `userEvent.click()` / `userEvent.type()` from \
                  `@testing-library/user-event`. `fireEvent` skips the \
                  intermediate events (keydown, keypress, input) that real \
                  browsers fire, so tests pass but miss event-handler bugs.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
