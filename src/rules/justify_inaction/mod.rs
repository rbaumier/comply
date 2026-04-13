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
//! A block is considered "justified" and NOT flagged iff it contains
//! at least one comment child (`line_comment` / `block_comment` for
//! Rust, `comment` for TS). That is the only accepted way to mark the
//! inaction intentional. A comment outside the block — on the line
//! above, trailing on the closing brace, etc. — is intentionally not
//! recognized: it keeps the rule simple and predictable, and placing
//! the explanation *inside* the braces makes it colocated with the
//! thing it explains.
//!
//! ## Scope exclusions
//!
//! The rule does NOT look at function / method / closure / arrow
//! bodies, nor at match arms whose value is a unit expression `()`
//! rather than a block `{}`. Empty function bodies are the standard
//! shape for stubs, trait marker impls, React/Vue no-op callbacks,
//! and similar, and flagging them would be pure noise.

mod rust;
#[cfg(test)]
mod shared_tests;
mod typescript;
mod vue;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "justify-inaction",
    description: "Empty catch/else/match-arm/loop block without an explaining comment inside.",
    remediation: "Add a comment inside the empty block explaining why the inaction is intentional, or remove the block if it is redundant.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
        .collect();
    backends.push((Language::Rust, Backend::TreeSitter(Box::new(rust::Check))));
    backends.push((Language::Vue, Backend::TreeSitter(Box::new(vue::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
