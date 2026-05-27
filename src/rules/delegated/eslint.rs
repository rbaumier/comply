//! ESLint core rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_and_clippy, oxlint_delegate};

// comply-ignore: max-function-lines — this is a flat data table, not logic; splitting it would scatter related rule entries across files for no readability gain.
pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "eqeqeq",
            "eqeqeq",
            Severity::Error,
            "Use === over == to avoid type coercion surprises.",
            "Replace `==` with `===` (and `!=` with `!==`). Loose equality \
             triggers implicit coercion rules that hide bugs.",
        ),
        entry(
            "no-var",
            "no-var",
            Severity::Error,
            "Never declare variables with `var`.",
            "Replace `var` with `const` (or `let` only when the binding \
             actually needs to be reassigned).",
        ),
        entry(
            "prefer-const",
            "prefer-const",
            Severity::Error,
            "Prefer `const` over `let` when the binding is never reassigned.",
            "Change `let` to `const` for bindings that are assigned once. \
             The intent becomes explicit and accidental reassignment becomes \
             a compile error.",
        ),
        entry_with_clippy(
            "no-else-return",
            "no-else-return",
            "clippy::redundant_else",
            Severity::Error,
            "Prefer guard clauses over else-after-return.",
            "Remove the `else` after a `return` and de-indent the trailing \
             block. Early returns keep the happy path at the leftmost level.",
        ),
        // `max-params` is handled natively — see `src/rules/max_params/`.
        // The native version exempts fixed-signature library callbacks
        // (TanStack Query `onError`/`queryFn`/etc.) and keeps the same
        // clippy delegation for Rust.
        entry_with_clippy(
            "max-depth",
            "max-depth",
            "clippy::excessive_nesting",
            Severity::Error,
            "Nesting beyond 2 levels is a smell.",
            "Flatten via early return, extract a helper, or invert the \
             condition. Deep nesting hides the happy path.",
        ),
        entry(
            "no-useless-catch",
            "no-useless-catch",
            Severity::Error,
            "A catch that only rethrows is pointless.",
            "If the catch block just rethrows the original error, remove it \
             — the error propagates identically without the ceremony.",
        ),
        // --- v1.1 additions ---
        // `id-length` is handled natively — see `src/rules/id_length/`.
        // `no-await-in-loop` handled natively — see src/rules/no_await_in_loop/.
        entry(
            "no-param-reassign",
            "no-param-reassign",
            Severity::Error,
            "Reassigning function parameters mutates the caller's data.",
            "Copy the argument into a local `let` if you need to mutate it. \
             Mutating params silently surprises callers.",
        ),
        entry(
            "no-empty",
            "no-empty",
            Severity::Error,
            "Empty blocks — including empty `catch` — must be justified.",
            "Either handle the case or add a comment naming why the block \
             is intentionally empty. Silent empty blocks rot into bugs.",
        ),
    ]
}

// Entry-builder helpers used by `register_all` above.

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
            categories: &["typescript"],
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
    severity: Severity,
    description: &'static str,
    remediation: &'static str,
) -> RuleDef {
    oxlint_and_clippy(
        RuleMeta {
            id,
            description,
            remediation,
            severity,
            doc_url: None,
            categories: &["typescript"],
        },
        oxlint_key,
        clippy_lint,
    )
}
