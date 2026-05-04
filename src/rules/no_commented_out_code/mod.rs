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

mod oxc_typescript;
mod rust;
#[cfg(test)]
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
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

pub(super) fn parses_as_typescript_code(body: &str) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(body, None) else {
        return false;
    };
    let root = tree.root_node();
    if root.has_error() {
        return false;
    }
    let mut found = false;
    crate::rules::walker::walk_tree(&tree, |node| {
        if found {
            return;
        }
        if matches!(
            node.kind(),
            "call_expression"
                | "assignment_expression"
                | "augmented_assignment_expression"
                | "lexical_declaration"
                | "variable_declaration"
                | "function_declaration"
                | "function_expression"
                | "generator_function_declaration"
                | "arrow_function"
                | "if_statement"
                | "for_statement"
                | "for_in_statement"
                | "while_statement"
                | "do_statement"
                | "return_statement"
                | "throw_statement"
                | "try_statement"
                | "switch_statement"
                | "class_declaration"
                | "interface_declaration"
                | "type_alias_declaration"
                | "enum_declaration"
                | "import_statement"
                | "export_statement"
                | "new_expression"
                | "update_expression"
                | "await_expression"
        ) {
            found = true;
        }
    });
    found
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn strip_line_comment() {
        assert_eq!(
            strip_comment_syntax("// let x = 1;").as_deref(),
            Some("let x = 1;")
        );
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
