//! prefer-classlist-toggle

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-classlist-toggle",
    description: "Prefer `Element#classList.toggle()` over conditional `add`/`remove`.",
    remediation: "Replace `if (c) el.classList.add('x') else el.classList.remove('x')` with `el.classList.toggle('x', c)`. The `toggle` method with a force argument is cleaner and avoids conditional branching.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
