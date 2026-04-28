//! eslint-plugin-unicorn rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{oxlint_and_clippy, RuleDef};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry_with_clippy(
            "unicorn/no-array-for-each",
            "unicorn/no-array-for-each",
            "clippy::needless_for_each",
            "Prefer `for...of` loops over `Array.forEach`.",
            "Replace `.forEach(x => ...)` with `for (const x of arr)`. \
             forEach can't break/continue and confuses async flows.",
        ),
        entry_with_clippy(
            "unicorn/prefer-array-flat-map",
            "unicorn/prefer-array-flat-map",
            "clippy::map_flatten",
            "Use `flatMap` instead of `map().flat()`.",
            "Chain `.flatMap(...)` once instead of `.map(...).flat()` — \
             one pass instead of two.",
        ),
    ]
}

// Entry-builder helpers used by `register_all` above.

/// Binds a rule to both an oxlint key and a clippy lint.
fn entry_with_clippy(
    id: &'static str,
    oxlint_key: &'static str,
    clippy_lint: &'static str,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_and_clippy(
        RuleMeta {
            id,
            description,
            remediation,
            severity: Severity::Error,
            doc_url: None, categories: &["typescript"],
        },
        oxlint_key,
        clippy_lint,
    )
}
