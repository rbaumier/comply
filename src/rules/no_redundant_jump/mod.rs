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

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod shared_tests;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-redundant-jump",
    description: "Redundant `return;` at end of function or `continue;` at end of loop body.",
    remediation: "Remove the redundant `return;` or `continue;` \u{2014} execution already falls through naturally.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
