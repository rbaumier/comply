//! eslint-plugin-unicorn rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{oxlint_and_clippy, oxlint_delegate, RuleDef, TS_FAMILY};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "unicorn/filename-case",
            "unicorn/filename-case",
            "Filenames should be kebab-case.",
            "Rename the file to kebab-case (`user-service.ts`, not \
             `userService.ts` or `user_service.ts`).",
        ),
        entry_with_clippy(
            "unicorn/no-array-for-each",
            "unicorn/no-array-for-each",
            "clippy::needless_for_each",
            "Prefer `for...of` loops over `Array.forEach`.",
            "Replace `.forEach(x => ...)` with `for (const x of arr)`. \
             forEach can't break/continue and confuses async flows. Rust: \
             `clippy::needless_for_each` flags `.iter().for_each(|x| ...)` \
             where a plain `for x in iter` is clearer.",
        ),
        entry_with_clippy(
            "unicorn/prefer-array-flat-map",
            "unicorn/prefer-array-flat-map",
            "clippy::map_flatten",
            "Use `flatMap` instead of `map().flat()`.",
            "Chain `.flatMap(...)` once instead of `.map(...).flat()` — \
             one pass instead of two. Rust: `clippy::map_flatten` flags \
             `.map(..).flatten()` and suggests `.flat_map(..)`.",
        ),
    ]
}

// Entry-builder helpers used by `register_all` above.

fn entry(
    id: &'static str,
    oxlint_key: &'static str,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_delegate(
        RuleMeta {
            id,
            description,
            remediation,
            severity: Severity::Error,
            doc_url: None, categories: &["typescript"],
        },
        oxlint_key,
        TS_FAMILY,
    )
}

/// Same shape as `entry()` but also binds the rule to a clippy lint on Rust.
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
