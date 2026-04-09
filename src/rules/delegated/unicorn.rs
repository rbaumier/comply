//! eslint-plugin-unicorn rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{oxlint_delegate, RuleDef, TS_FAMILY};

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
            doc_url: None,
        },
        oxlint_key,
        TS_FAMILY,
    )
}

pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "unicorn/filename-case",
            "unicorn/filename-case",
            "Filenames should be kebab-case.",
            "Rename the file to kebab-case (`user-service.ts`, not \
             `userService.ts` or `user_service.ts`).",
        ),
        entry(
            "unicorn/no-array-for-each",
            "unicorn/no-array-for-each",
            "Prefer `for...of` loops over `Array.forEach`.",
            "Replace `.forEach(x => ...)` with `for (const x of arr)`. \
             forEach can't break/continue and confuses async flows.",
        ),
        entry(
            "unicorn/prefer-array-flat-map",
            "unicorn/prefer-array-flat-map",
            "Use `flatMap` instead of `map().flat()`.",
            "Chain `.flatMap(...)` once instead of `.map(...).flat()` — \
             one pass instead of two.",
        ),
    ]
}
