//! ESLint core rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{oxlint_and_clippy, oxlint_delegate, RuleDef, TS_FAMILY};

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
            "curly",
            "curly",
            Severity::Error,
            "Require curly braces on every control-flow block.",
            "Wrap single-statement bodies in `{ ... }`. Missing braces make \
             future edits error-prone.",
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
             block. Early returns keep the happy path at the leftmost level. \
             Rust: enabled via `clippy::redundant_else` (also see \
             `clippy::needless_return`).",
        ),
        entry(
            "no-magic-numbers",
            "no-magic-numbers",
            Severity::Error,
            "Extract literal numbers into named constants.",
            "Move the literal into a named constant at module scope. If the \
             literal is a one-shot index (0, 1, -1), it's ignored.",
        ),
        entry_with_clippy(
            "max-params",
            "max-params",
            "clippy::too_many_arguments",
            Severity::Error,
            "Functions should take at most 3 positional arguments.",
            "If you need more than 3 parameters, pack them into an options \
             object — named fields carry intent where positional arguments \
             don't. Rust: enable `clippy::too_many_arguments` and set \
             `too-many-arguments-threshold = 3` in `clippy.toml`.",
        ),
        entry_with_clippy(
            "max-depth",
            "max-depth",
            "clippy::excessive_nesting",
            Severity::Error,
            "Nesting beyond 2 levels is a smell.",
            "Flatten via early return, extract a helper, or invert the \
             condition. Deep nesting hides the happy path. Rust: enable \
             `clippy::excessive_nesting` (restriction lint) and set \
             `excessive-nesting-threshold = 3` in `clippy.toml`.",
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
        entry_with_clippy(
            "id-length",
            "id-length",
            "clippy::min_ident_chars",
            Severity::Error,
            "Single-letter identifiers hide intent.",
            "Rename to a full word — `createdAt` not `d`, `userCount` not `n`. \
             Exceptions: loop indices `i`, `j` inside tight for-loops. \
             Rust: enable `clippy::min_ident_chars` in your crate.",
        ),
        entry(
            "no-await-in-loop",
            "no-await-in-loop",
            Severity::Error,
            "Sequential `await` in a loop serializes independent work.",
            "If the iterations don't depend on each other, use \
             `Promise.all(items.map(f))` instead. If they do depend, keep the \
             loop and document why.",
        ),
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
        entry(
            "no-implicit-coercion",
            "no-implicit-coercion",
            Severity::Error,
            "Implicit type coercion hides intent.",
            "Replace `!!value` with `Boolean(value)`, `+str` with \
             `Number(str)`, `~~n` with `Math.trunc(n)`. Explicit is clearer.",
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
            doc_url: None, categories: &["typescript"],
        },
        oxlint_key,
        clippy_lint,
    )
}
