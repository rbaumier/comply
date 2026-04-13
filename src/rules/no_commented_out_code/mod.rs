//! no-commented-out-code — delete dead comments, git history keeps originals.
//!
//! ## Why this rule was rewritten
//!
//! The previous implementation was a text heuristic: a `//` comment was
//! flagged if its body started with a code keyword (`const`, `let`, …)
//! AND contained at least two "code-shaped" punctuation characters
//! (`;`, `=`, `{`, `}`, `(`, `)`). That flagged prose like
//! `// const foo =, let foo =, var foo =` — three `=` is enough
//! punctuation, `const` is a keyword, verdict "commented code". It's
//! not: it's a pattern list enumerating declaration shapes, each one
//! incomplete (`=` with no RHS).
//!
//! The distinction that the old heuristic missed is the core one: real
//! commented-out code is complete, parseable syntax. Prose-describing-
//! syntax is a salad of tokens that cannot compile.
//!
//! ## How the new rule works
//!
//! 1. **Collect `comment` nodes** from the already-parsed AST (TS) or
//!    `line_comment` + `block_comment` nodes (Rust). Uses a manual
//!    cursor walk — `walker::walk_tree`'s closure has a higher-ranked
//!    lifetime that prevents us from pushing nodes into a Vec, so we
//!    re-do the walk locally to preserve the tree lifetime on each
//!    node.
//!
//! 2. **Group consecutive comments** into virtual blocks. Two comments
//!    are adjacent if the second one starts on the same row as, or
//!    the row right after, the first one ends. A block of three `//`
//!    lines commenting out a three-line function is analyzed as one
//!    3-line body, not three 1-line bodies — the single-line view
//!    would never parse successfully.
//!
//! 3. **Strip the delimiters** (`//`, `/*`, `*/`). Doc comments
//!    (`///`, `//!`, `/**`, `/*!`) are dropped up front: they
//!    legitimately carry example snippets, and `/** @returns cost */`
//!    is absolutely not dead code.
//!
//! 4. **Fast filter**: the joined body must contain `;` or `{`. Pure
//!    prose almost never does. This short-circuits the common case
//!    without paying for a parse.
//!
//! 5. **Re-parse the body** with the SAME tree-sitter grammar the
//!    outer file was parsed with. The Rust side wraps the body in
//!    `fn __probe__() { ... }` because most commented-out Rust is
//!    statement-level and a bare `let x = 5;` is a hard parse error
//!    at module scope; wrapped, it's a legal function body.
//!
//! 6. **Verdict**: the re-parse must have zero errors (`!root.has_error()`)
//!    AND contain at least one "rich" node — a declaration, a call,
//!    an assignment, a control-flow structure. Bare prose like
//!    `hello world.` parses as two expression_statements of
//!    identifiers with no rich children and is NOT flagged.
//!
//! ## Language coverage
//!
//! - **TS / JS / TSX (React)**: handled by the `typescript` backend.
//!   React components — JSX and TSX — go through the TSX grammar and
//!   are covered by the same backend.
//! - **Rust**: handled by the `rust` backend.
//! - **Vue**: handled by the `vue` backend. The Vue SFC is parsed
//!   with `tree-sitter-vue-updated`, each `<script>` block's
//!   `raw_text` is extracted via `crate::rules::vue_sfc`, and the
//!   text is re-parsed with the TS grammar. The same comment-grouping
//!   and mini-parse logic runs on the inner TS tree, and diagnostic
//!   `(row, column)` values are translated back to Vue file
//!   coordinates before being emitted. HTML comments inside
//!   `<template>` are NOT inspected — they rarely carry JavaScript.
//!
//! ## Intentional false negatives
//!
//! - A commented block where the first `//` line starts inside a
//!   multi-line comment region that `walk_tree`'s error recovery
//!   decided to skip will be missed. This is rare and matches the
//!   global error-subtree skip policy.
//! - `/* const x = 5 */` without a trailing `;` does parse cleanly
//!   in TS (ASI) and DOES flag. This is intentional.
//! - `//  const  x  =  5 ;  ` (excess whitespace) flags. Intentional.

mod rust;
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-commented-out-code",
    description: "Commented-out code is unreviewable, unreachable, and rots.",
    remediation: "Delete the commented-out code. Git history preserves the \
                  original if you need to recover it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};

pub fn register() -> RuleDef {
    // Built manually (instead of `register_ts_family_with_rust!`) so we
    // can attach a dedicated Vue backend on top of the TS/JS/TSX/Rust
    // set. The Vue backend walks the outer Vue SFC tree, extracts
    // `<script>` blocks, and re-parses their contents with the TS
    // grammar before applying the same NCOC logic.
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}

/// Strip the comment delimiters (`//`, `/*`, `*/`) from a raw comment
/// node's text. Returns `None` for doc comments (`///`, `//!`, `/**`,
/// `/*!`), which are NOT candidates for "commented-out code" because
/// they routinely carry example snippets.
pub(super) fn strip_comment_syntax(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.starts_with("///")
        || raw.starts_with("//!")
        || raw.starts_with("/**")
        || raw.starts_with("/*!")
    {
        return None;
    }
    if let Some(body) = raw.strip_prefix("//") {
        return Some(body.trim_start().to_string());
    }
    if let Some(body) = raw.strip_prefix("/*") {
        let body = body.strip_suffix("*/").unwrap_or(body);
        return Some(body.to_string());
    }
    None
}

/// Fast filter: a block of text is only worth re-parsing if it contains
/// at least one structural marker. Pure prose rarely contains `;` or `{`.
pub(super) fn has_code_shape(text: &str) -> bool {
    text.contains(';') || text.contains('{')
}

/// Collect every node whose kind is in `kinds` into a Vec whose
/// lifetime matches the tree. Can't use `walker::walk_tree` for this
/// because its closure argument has a higher-ranked lifetime and
/// nodes can't escape the closure; the manual cursor walk here
/// preserves the tree lifetime `'t` on each node we push.
///
/// Matches `walk_tree`'s error-skipping semantics: subtrees rooted
/// at ERROR or MISSING nodes are skipped entirely so we don't pull
/// phantom comment nodes out of a parse-error region.
pub(super) fn collect_nodes_of_kinds<'t>(
    tree: &'t tree_sitter::Tree,
    kinds: &[&'static str],
) -> Vec<tree_sitter::Node<'t>> {
    let mut out: Vec<tree_sitter::Node<'t>> = Vec::new();
    let mut cursor = tree.walk();
    'outer: loop {
        let node = cursor.node();
        let bad = node.is_error() || node.is_missing();
        if !bad {
            if kinds.contains(&node.kind()) {
                out.push(node);
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'outer;
            }
            if !cursor.goto_parent() {
                return out;
            }
        }
    }
}

/// Group adjacent comment nodes into virtual blocks. Two comments are
/// considered adjacent if the second one starts on the same line as,
/// or the line immediately after, the first one ends.
pub(super) fn group_adjacent<'tree>(
    comments: &[tree_sitter::Node<'tree>],
) -> Vec<Vec<tree_sitter::Node<'tree>>> {
    let mut groups: Vec<Vec<tree_sitter::Node<'tree>>> = Vec::new();
    for &c in comments {
        let extend = groups
            .last()
            .and_then(|g| g.last())
            .is_some_and(|last| c.start_position().row <= last.end_position().row + 1);
        if extend {
            groups.last_mut().expect("last group exists").push(c);
        } else {
            groups.push(vec![c]);
        }
    }
    groups
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn strip_line_comment() {
        assert_eq!(strip_comment_syntax("// let x = 1;").as_deref(), Some("let x = 1;"));
    }

    #[test]
    fn strip_block_comment() {
        assert_eq!(strip_comment_syntax("/* foo */").as_deref(), Some(" foo "));
    }

    #[test]
    fn reject_triple_slash() {
        assert!(strip_comment_syntax("/// doc").is_none());
    }

    #[test]
    fn reject_double_star() {
        assert!(strip_comment_syntax("/** jsdoc */").is_none());
    }

    #[test]
    fn reject_inner_module_doc() {
        assert!(strip_comment_syntax("//! inner").is_none());
    }

    #[test]
    fn has_code_shape_semicolon() {
        assert!(has_code_shape("foo();"));
    }

    #[test]
    fn has_code_shape_brace() {
        assert!(has_code_shape("if (x) {"));
    }

    #[test]
    fn has_no_code_shape_in_prose() {
        assert!(!has_code_shape("the quick brown fox jumps"));
    }
}
