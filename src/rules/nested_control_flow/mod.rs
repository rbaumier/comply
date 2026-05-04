//! nested-control-flow — flag control-flow nesting deeper than `MAX_DEPTH` (3).
//!
//! ## Depth counting
//!
//! For each control-flow node (if/for/while/match/switch/try/loop/do), the
//! depth is the number of control-flow ancestors plus one for the node
//! itself. Only control-flow nodes count — non-control blocks (plain
//! `block`, `impl_item`, `mod_item`, object literals, class bodies…) are
//! transparent, consistent with eslint `max-depth`.
//!
//! ## Function boundaries
//!
//! Depth resets at the nearest callable boundary. In Rust that means
//! `function_item` and `closure_expression`; in TS/JS, any of
//! `function_declaration`, `function_expression`, `arrow_function`,
//! `method_definition`, `generator_function(_declaration)`. Each function
//! body has its own cognitive scope — outer nesting does not leak into a
//! closure passed to a combinator.
//!
//! ## `else if` cascades
//!
//! Both tree-sitter-rust and tree-sitter-typescript parse `else if` as
//! an `if_*` nested inside the parent's `else_clause` — syntactically it
//! is another nesting level, visually it is a flat cascade. The backend
//! collapses the cascade: when walking ancestors, an `if_*` reached via
//! its own `else_clause` child is skipped, and the inner `if_*` of an
//! `else if` does not report on its own. This mirrors eslint `max-depth`,
//! whose ESTree equivalent is `if (node.parent.type !== "IfStatement")
//! { pushBlock(node); }`.

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod shared_tests;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{Language, RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "nested-control-flow",
    description: "Deeply nested control flow (depth > 3) is hard to read and maintain.",
    remediation: "Extract inner blocks into separate functions, use early returns or guard clauses to reduce nesting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
        .collect();
    backends.push((Language::Rust, Backend::TreeSitter(Box::new(rust::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
