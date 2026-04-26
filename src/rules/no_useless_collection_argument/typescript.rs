//! no-useless-collection-argument AST backend — flag `new Set([])`,
//! `new Map(undefined)`, `new WeakSet(null)`, etc.
//!
//! Walks `new_expression` nodes whose constructor is one of `Set`, `Map`,
//! `WeakSet`, `WeakMap` and whose single argument is an empty/nullish
//! literal: `[]`, `undefined`, `null`, `""`, `''`, or `` `` ``.

use crate::diagnostic::{Diagnostic, Severity};

const COLLECTIONS: &[&str] = &["Set", "Map", "WeakSet", "WeakMap"];

/// Classify the single argument node, returning a human-friendly label
/// for the diagnostic message when it's a useless value, or `None`.
fn useless_arg_label(arg: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    match arg.kind() {
        "array" => {
            // Useless only when the array literal has no elements.
            let mut cursor = arg.walk();
            if arg.named_children(&mut cursor).next().is_none() {
                Some("empty array")
            } else {
                None
            }
        }
        "undefined" => Some("`undefined`"),
        "null" => Some("`null`"),
        "string" | "template_string" => {
            // An empty string literal has no string_fragment / template_chars
            // children; quotes are anonymous tokens.
            let mut cursor = arg.walk();
            let has_content = arg.named_children(&mut cursor).any(|c| {
                let k = c.kind();
                k == "string_fragment" || k == "template_substitution" || k == "escape_sequence"
            });
            // Some grammars expose template chars as anonymous; check raw bytes.
            if !has_content {
                let bytes = &source[arg.byte_range()];
                // Strip quotes/backticks; if nothing remains, it's empty.
                if bytes.len() >= 2 {
                    let inner = &bytes[1..bytes.len() - 1];
                    if inner.is_empty() {
                        return Some("empty string");
                    }
                }
            }
            None
        }
        // `identifier` named `undefined` (older grammar): handled above as "undefined";
        // tree-sitter-typescript exposes `undefined` as its own kind.
        _ => None,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "new_expression" {
        return;
    }
    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    let Ok(name) = std::str::from_utf8(&source[constructor.byte_range()]) else { return };
    if !COLLECTIONS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let named: Vec<tree_sitter::Node> = args.named_children(&mut cursor).collect();
    if named.len() != 1 {
        return;
    }
    let Some(label) = useless_arg_label(named[0], source) else { return };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("The {label} argument is useless — remove it."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_new_set_empty_array() {
        let d = run_on("const s = new Set([]);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty array"));
    }

    #[test]
    fn flags_new_map_undefined() {
        let d = run_on("const m = new Map(undefined);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`undefined`"));
    }

    #[test]
    fn flags_new_weakset_null() {
        let d = run_on("const ws = new WeakSet(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`null`"));
    }

    #[test]
    fn flags_new_set_empty_string() {
        let d = run_on("const s = new Set(\"\");");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty string"));
    }

    #[test]
    fn allows_new_set_with_values() {
        assert!(run_on("const s = new Set([1, 2, 3]);").is_empty());
    }

    #[test]
    fn allows_new_set_no_args() {
        assert!(run_on("const s = new Set();").is_empty());
    }

    #[test]
    fn allows_new_map_with_entries() {
        assert!(run_on("const m = new Map([[\"a\", 1]]);").is_empty());
    }
}
