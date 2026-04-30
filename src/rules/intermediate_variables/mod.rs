//! intermediate-variables — flag `if` conditions that chain three or
//! more boolean operands via `&&` / `||` / `??`.
//!
//! The remediation is to extract one or more parts of the chain into
//! named local variables so the `if` reads as a couple of high-level
//! checks rather than a flat conjunction of five low-level predicates.
//!
//! ## Detection shape
//!
//! - Walk `if_expression` (Rust) / `if_statement` (TS).
//! - Take the `condition` field.
//! - Walk the condition subtree and count `binary_expression` nodes
//!   whose `operator` text is `&&`, `||`, or (TS only) `??`. Comparison
//!   ops (`==`, `!=`, `<`, `>`, `===`, `!==`, …) and arithmetic ops do
//!   NOT contribute — they live INSIDE a single condition, not BETWEEN
//!   conditions.
//! - Flag when the count is ≥ 2 (three or more chained operands).
//! - Stop the walk at nested callable boundaries (`closure_expression` /
//!   `function_item` in Rust; `function_declaration` / `function_expression`
//!   / `arrow_function` / `method_definition` / `generator_function` in
//!   TS) so that lambda predicates passed to combinators
//!   (`.filter(|x| x.a && x.b && x.c)`) don't contribute to the
//!   enclosing `if`'s count.
//!
//! The rule never looks at `call_expression` or `return_expression`.
//! A long arithmetic expression in a function call argument, a chained
//! iterator call, or a complex return value are all out of scope — the
//! readability problem this rule targets is specifically a flat chain
//! of booleans in a branch decision.

mod rust;
#[cfg(test)]
mod shared_tests;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "intermediate-variables",
    description: "`if` condition chains three or more boolean operands.",
    remediation: "Extract parts of the condition into named local variables so the `if` reads as one or two high-level checks.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
