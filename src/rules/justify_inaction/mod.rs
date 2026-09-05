//! justify-inaction — flag empty control-flow blocks with no comment
//! inside explaining why the inaction is intentional.
//!
//! ## Targets
//!
//! **TypeScript / JavaScript / TSX / Vue `<script>`**:
//! - `catch_clause` with empty `body` — silent error swallow.
//! - `finally_clause` with empty `body` — pointless finally.
//! - `if_statement` with empty `consequence`.
//! - `else_clause` with empty `statement_block`.
//! - `switch_default` with empty (or absent) body.
//! - `while_statement` / `do_statement` / `for_statement` /
//!   `for_in_statement` / `for_of_statement` with empty body.
//!
//! **Rust**:
//! - `if_expression` with empty `consequence`.
//! - `else_clause` with empty `block`.
//! - `match_arm` whose `value` is an empty `block` — the canonical
//!   `None => {}`, `Err(_) => {}`, `_ => {}` silent-ignore shapes.
//! - `for_expression` / `while_expression` / `loop_expression` with
//!   empty body.
//!
//! ## Justification mechanism
//!
//! A block is "justified" and NOT flagged when it contains at least one
//! comment child (`line_comment` / `block_comment` for Rust, `comment` for
//! TS) — the explanation sits inside the braces, colocated with the thing it
//! explains. A comment elsewhere (trailing on the closing brace, say) is not
//! read as a justification, with one exception noted in the Rust backend: a
//! comment on the line directly above a loop.
//!
//! A block also needs no comment when the code around it already says why it
//! is empty, and only a structural property may establish that. One such
//! property holds in every backend: a `while` / `do…while` whose condition
//! contains a call is a drain / register-polling loop — the condition drives
//! every iteration, so a body comment could only restate it. The rest are
//! shape-keyed exemptions documented in each backend module.
//!
//! ## Scope exclusions
//!
//! The rule does NOT look at function / method / closure / arrow
//! bodies, nor at match arms whose value is a unit expression `()`
//! rather than a block `{}`. Empty function bodies are the standard
//! shape for stubs, trait marker impls, React/Vue no-op callbacks,
//! and similar, and flagging them would be pure noise.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod shared_tests;
mod vue;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "justify-inaction",
    description: "Empty catch/else/match-arm/loop block without an explaining comment inside.",
    remediation: "Add a comment inside the empty block explaining why the inaction is intentional, or remove the block if it is redundant.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
        .collect();
    backends.push((Language::Rust, Backend::TreeSitter(Box::new(rust::Check))));
    backends.push((Language::Vue, Backend::TreeSitter(Box::new(vue::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
