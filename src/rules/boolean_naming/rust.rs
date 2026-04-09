//! boolean-naming backend for Rust.
//!
//! Why: the skill rule "booleans must start with is/has/should/can/will/did/was"
//! applies to Rust too, using snake_case conventions (`is_ready`, `has_items`).
//! Clippy has no equivalent — this is a comply-specific opinionated check.
//!
//! Detection: walk `let_declaration` and `parameter` nodes whose type is
//! `bool` (via `primitive_type` child) or whose initializer is a
//! `boolean_literal`. Check the identifier against the valid prefix list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const VALID_PREFIXES: &[&str] = &[
    "is_", "has_", "should_", "can_", "will_", "did_", "was_",
];
const NEGATIVE_SUBSTRINGS: &[&str] = &["_not_", "isnt_", "cannot_", "shouldnt_"];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if let Some(d) = check_node(node, source_bytes, ctx.path) {
                diagnostics.push(d);
            }
        });
        diagnostics
    }
}

fn check_node(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
) -> Option<Diagnostic> {
    if node.kind() != "let_declaration" && node.kind() != "parameter" {
        return None;
    }
    if !has_boolean_type_or_value(node, source) {
        return None;
    }
    let name = extract_identifier(node, source)?;
    let problem = classify_name(name)?;
    let pos = node.start_position();
    Some(Diagnostic {
        path: path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "boolean-naming".into(),
        message: format!(
            "Boolean '{name}' {problem}. Use a predicate prefix: \
             `is_*`, `has_*`, `should_*`, `can_*`, `will_*`, `did_*`, `was_*`."
        ),
        severity: Severity::Warning,
    })
}

/// True if the let_declaration / parameter has `: bool` annotation or is
/// initialized with a boolean literal.
fn has_boolean_type_or_value(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "primitive_type" => {
                if child.utf8_text(source).is_ok_and(|t| t == "bool") {
                    return true;
                }
            }
            "boolean_literal" => return true,
            _ => {}
        }
    }
    false
}

fn extract_identifier<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// Return a short problem description if the name violates the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    if NEGATIVE_SUBSTRINGS.iter().any(|neg| name.contains(neg)) {
        return Some("is negatively phrased — use the positive form with `!`");
    }
    for &prefix in VALID_PREFIXES {
        if name.starts_with(prefix) {
            return None;
        }
    }
    Some("is missing a predicate prefix")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn allows_is_prefix() {
        assert!(run_on("fn f() { let is_ready: bool = true; }").is_empty());
    }

    #[test]
    fn allows_has_prefix() {
        assert!(run_on("fn f() { let has_items = true; }").is_empty());
    }

    #[test]
    fn flags_missing_prefix_with_annotation() {
        let diags = run_on("fn f() { let ready: bool = true; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'ready'"));
    }

    #[test]
    fn flags_inferred_boolean() {
        assert_eq!(run_on("fn f() { let ready = true; }").len(), 1);
    }

    #[test]
    fn flags_param_without_prefix() {
        let diags = run_on("fn f(ready: bool) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_flag_non_boolean() {
        assert!(run_on("fn f() { let name: String = String::new(); }").is_empty());
    }
}
