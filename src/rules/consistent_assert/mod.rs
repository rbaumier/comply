//! consistent-assert — flag `assert(x === y)` → `assert.strictEqual(x, y)` with node:assert.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "consistent-assert",
    description: "Prefer `assert.ok(…)` over bare `assert(…)` with `node:assert`.",
    remediation: "Replace bare `assert(…)` calls with `assert.ok(…)` for consistency with the `node:assert` API.",
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
