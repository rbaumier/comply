//! typescript-eslint plugin rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{oxlint_delegate, RuleDef, TS_FAMILY};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "typescript/no-explicit-any",
            "typescript/no-explicit-any",
            Severity::Error,
            "Using `any` defeats the type system.",
            "Replace `any` with a concrete type. When the shape is genuinely \
             unknown at the boundary, use `unknown` and narrow it before use.",
        ),
        entry(
            "typescript/no-unsafe-type-assertion",
            "typescript/no-unsafe-type-assertion",
            Severity::Error,
            "Unsafe `as` assertions bypass the type checker.",
            "Replace the assertion with a proper type guard or narrow via \
             runtime validation before treating the value as the target type.",
        ),
        entry(
            "typescript/array-type",
            "typescript/array-type",
            Severity::Error,
            "Use `T[]` consistently for arrays.",
            "Prefer `T[]` over `Array<T>`. Mixing the two styles creates \
             pointless review churn.",
        ),
        entry(
            "typescript/consistent-type-imports",
            "typescript/consistent-type-imports",
            Severity::Error,
            "Import types with `import type` so the bundler can strip them.",
            "Prefix the import with `import type` when every binding is only \
             used as a type. This lets the bundler elide the import entirely.",
        ),
        entry(
            "typescript/no-non-null-assertion",
            "typescript/no-non-null-assertion",
            Severity::Error,
            "The `!` non-null assertion hides potential runtime errors.",
            "Replace `foo!` with an explicit null check or narrow the type \
             via a type guard. `!` silently bypasses the type system.",
        ),
        entry(
            "typescript/prefer-as-const",
            "typescript/prefer-as-const",
            Severity::Error,
            "Use `as const` to pin literal types.",
            "Replace `as 'literal'` with `as const` — more concise and \
             preserves the literal type across refactors.",
        ),
        entry(
            "typescript/prefer-ts-expect-error",
            "typescript/prefer-ts-expect-error",
            Severity::Error,
            "Use `@ts-expect-error` instead of `@ts-ignore`.",
            "Replace `@ts-ignore` with `@ts-expect-error`. The latter errors \
             when the suppressed issue is fixed, preventing bit-rot.",
        ),
        entry(
            "typescript/no-unsafe-function-type",
            "typescript/no-unsafe-function-type",
            Severity::Error,
            "The bare `Function` type accepts any signature.",
            "Replace `Function` with a specific function type like \
             `(arg: X) => Y`. Bare `Function` offers no type safety.",
        ),
        entry(
            "typescript/no-require-imports",
            "typescript/no-require-imports",
            Severity::Error,
            "Use ES module imports, not CommonJS `require`.",
            "Replace `const x = require('x')` with `import x from 'x'`. \
             require() bypasses the type system and tree-shaking.",
        ),
    ]
}

// Entry-builder helper used by `register_all` above.

fn entry(
    id: &'static str,
    oxlint_key: &'static str,
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_delegate(
        RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
        },
        oxlint_key,
        TS_FAMILY,
    )
}
