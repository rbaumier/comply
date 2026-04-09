//! boolean-naming backend — booleans must start with `is`/`has`/`should`/`can`.
//!
//! The prefix conveys that the name is a predicate — `isReady`, `hasItems`,
//! `shouldRetry`, `canEdit`. Without it, readers don't know whether `valid`
//! is a boolean or a validation error struct. We also accept the positive
//! form only — `isNotReady` is banned in favor of `!isReady`.
//!
//! Detection: walk `variable_declarator` and `required_parameter` nodes
//! whose `type_annotation` child is `: boolean`. Also handle the form
//! `const x = true|false` where the type is inferred.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const VALID_PREFIXES: &[&str] = &["is", "has", "should", "can", "will", "did", "was"];
const NEGATIVE_SUBSTRINGS: &[&str] = &["Not", "Isnt", "Cannot", "Cant", "Shouldnt"];

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

/// Check a single AST node. Returns a diagnostic if it's a boolean-typed
/// binding whose name doesn't start with an accepted prefix (or uses a
/// negative form).
fn check_node(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
) -> Option<Diagnostic> {
    if node.kind() != "variable_declarator" && node.kind() != "required_parameter" {
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
             `is*`, `has*`, `should*`, `can*`, `will*`, `did*`, `was*`."
        ),
        severity: Severity::Warning,
    })
}

/// True if the declarator/parameter has `: boolean` annotation or is
/// initialized with `true`/`false` literal.
fn has_boolean_type_or_value(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_annotation" => {
                if type_annotation_is_boolean(child, source) {
                    return true;
                }
            }
            "true" | "false" => return true,
            _ => {}
        }
    }
    false
}

/// Returns true when the type annotation's payload is `boolean`.
fn type_annotation_is_boolean(type_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = type_node.walk();
    for child in type_node.children(&mut cursor) {
        if child.kind() == "predefined_type"
            && child.utf8_text(source).is_ok_and(|t| t.trim() == "boolean")
        {
            return true;
        }
    }
    false
}

/// Return the first identifier child's text, if any.
fn extract_identifier<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// Return a short problem description if the name doesn't match the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    if NEGATIVE_SUBSTRINGS.iter().any(|neg| name.contains(neg)) {
        return Some("is negatively phrased — use the positive form with `!`");
    }
    for &prefix in VALID_PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix) {
            // Require a word boundary — `issuer` shouldn't count as starting with `is`.
            if rest.is_empty() || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                return None;
            }
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
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx {
                path: Path::new("t.ts"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn allows_is_prefix() {
        assert!(run_on("const isReady: boolean = true;").is_empty());
    }

    #[test]
    fn allows_has_prefix() {
        assert!(run_on("const hasItems: boolean = false;").is_empty());
    }

    #[test]
    fn allows_should_will_did_was() {
        for name in ["shouldRetry", "willSucceed", "didFire", "wasLoaded"] {
            let source = format!("const {name} = true;");
            assert!(run_on(&source).is_empty(), "'{name}' should be allowed");
        }
    }

    #[test]
    fn flags_missing_prefix_with_annotation() {
        let diags = run_on("const ready: boolean = true;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'ready'"));
    }

    #[test]
    fn flags_inferred_boolean() {
        let diags = run_on("const ready = true;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_negative_phrasing() {
        let diags = run_on("const isNotReady = false;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("negatively"));
    }

    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // `issuer` starts with `is` but is not a boolean predicate.
        // It won't be flagged because its type isn't boolean.
        assert!(run_on("const issuer: string = 'ACME';").is_empty());
    }

    #[test]
    fn flags_param_without_prefix() {
        let diags = run_on("function f(ready: boolean) {}");
        assert_eq!(diags.len(), 1);
    }
}
