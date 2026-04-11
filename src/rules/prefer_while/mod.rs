//! prefer-while

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-while",
    description: "`for (;;)` or `for (;cond;)` without init/update — use `while` instead.",
    remediation: "Replace `for (;;)` with `while (true)` and `for (;condition;)` with `while (condition)`. The `for` form hides intent when init and update are unused.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
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
