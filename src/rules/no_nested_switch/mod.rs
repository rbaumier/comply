//! no-nested-switch — flag `switch` statements nested inside another `switch`.
//!
//! ## Scope: TypeScript / JavaScript / TSX only.
//!
//! JS/TS `switch` is genuinely error-prone: implicit fall-through between
//! cases, no exhaustiveness check, lexical scope shared across cases. Nesting
//! one inside another compounds every one of these foot-guns, and extracting
//! the inner `switch` into a helper is almost always a clear win.
//!
//! ## Why no Rust backend
//!
//! Rust `match` has none of those properties: it is exhaustive, has no
//! fall-through, and each arm is its own scope. It is the idiomatic dispatch
//! construct of the language — nesting a `match` inside one arm of a parent
//! `match` is a normal way to refine a sub-case (e.g. outer matches an enum
//! variant, inner matches a downstream value extracted from that variant).
//! Mechanically flagging every nested `match` produces noise on idiomatic
//! code and pushes users toward awkward flattening or premature extraction.
//!
//! When nested matches actually become hard to follow, it is because the
//! overall function is too complex — a signal already captured by
//! `cognitive-complexity` and `cyclomatic-complexity`, both of which have
//! Rust backends. There is no useful nesting threshold for `match` that
//! would not duplicate or contradict those rules.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-nested-switch",
    description: "`switch` inside another `switch` is hard to follow.",
    remediation: "Extract the inner switch into a separate function. Nested switches create deeply indented, hard-to-read code that is easy to get wrong.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
