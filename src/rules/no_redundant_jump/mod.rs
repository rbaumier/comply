//! no-redundant-jump — flag bare `return;` or `continue;` at the tail
//! of the enclosing callable / loop.
//!
//! A jump is redundant iff, walking up from it through tail positions
//! only, we reach a function boundary (for `return;`) or a loop
//! boundary (for `continue;`). At every `block`/`statement_block` step
//! the current node must be the last named child; `if` / `else` /
//! `match` / `switch_case` wrappers are transparent because every
//! branch is a parallel tail.
//!
//! `return;` with a value (`return 42;`) and labeled `continue label;`
//! are skipped — only bare forms are considered, since the with-value
//! variants are not style nits. `break;` is out of scope: break is
//! rarely the last statement of a loop and carries switch fall-through
//! implications in JS.

mod rust;
#[cfg(test)]
mod shared_tests;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-jump",
    description: "Redundant `return;` at end of function or `continue;` at end of loop body.",
    remediation: "Remove the redundant `return;` or `continue;` \u{2014} execution already falls through naturally.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
