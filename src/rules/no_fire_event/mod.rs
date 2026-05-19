//! no-fire-event — prefer `userEvent.click` over `fireEvent.click` in tests.
//!
//! Flags `fireEvent.click(...)` calls inside test files and suggests
//! `userEvent.click(...)` instead, which dispatches the full pointer/focus
//! sequence a real browser would fire.
//!
//! Other `fireEvent.*` methods (`focus`, `blur`, `keyDown`, `change`,
//! `pointerDown`, ...) are intentionally not flagged: their `userEvent`
//! counterparts either do not exist or add extra events that defeat tests
//! targeting low-level focus/keyboard/debounce behaviour.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-fire-event",
    description: "`fireEvent.click` dispatches a single synthetic click — `userEvent.click` reproduces the full pointer/focus sequence a real browser fires.",
    remediation: "Replace `fireEvent.click(el)` with `userEvent.click(el)` \
                  from `@testing-library/user-event`. Other `fireEvent.*` \
                  methods (`focus`, `blur`, `keyDown`, `change`, pointer \
                  events, ...) are left alone — they have no clean \
                  `userEvent` equivalent and are the right tool for testing \
                  low-level focus, keyboard or debounce behaviour.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
